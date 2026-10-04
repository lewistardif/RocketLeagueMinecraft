#include "havok_world.h"
#include <chrono>
#include <unordered_set>

namespace havok {

namespace {

constexpr int kMaxKeys = 16384;
constexpr size_t kMaxTris = 400000;

bool finite(const float* v, int n) {
	for (int i = 0; i < n; i++)
		if (!std::isfinite(v[i]) || std::fabs(v[i]) > 1.0e7f) return false;
	return true;
}

// Havok transform: rotation columns at [0..2], [4..6], [8..10]; translation at [12..14].
void xfPoint(const float* xf, const float* p, float* out) {
	for (int i = 0; i < 3; i++) out[i] = xf[i] * p[0] + xf[4 + i] * p[1] + xf[8 + i] * p[2] + xf[12 + i];
}

void xfDir(const float* xf, const float* d, float* out) {
	for (int i = 0; i < 3; i++) out[i] = xf[i] * d[0] + xf[4 + i] * d[1] + xf[8 + i] * d[2];
}

void xfCompose(const float* parent, const float* child, float* out) {
	for (int c = 0; c < 3; c++) {
		xfDir(parent, child + c * 4, out + c * 4);
		out[c * 4 + 3] = 0;
	}
	xfPoint(parent, child + 12, out + 12);
	out[15] = 1;
}

bool xfLooksValid(const float* xf) {
	if (!finite(xf, 16)) return false;
	for (int c = 0; c < 3; c++) {
		const float* col = xf + c * 4;
		if (std::fabs(col[0] * col[0] + col[1] * col[1] + col[2] * col[2] - 1.0f) > 0.05f) return false;
	}
	return true;
}

const float* vec(const void* base, size_t offset) { return reinterpret_cast<const float*>(reinterpret_cast<const uint8_t*>(base) + offset); }

template <class T>
T field(const void* base, size_t offset) {
	T v;
	std::memcpy(&v, reinterpret_cast<const uint8_t*>(base) + offset, sizeof(T));
	return v;
}

void aabbOf(const RE::hkAabb& box, float* lo, float* hi) {
	alignas(16) float mn[4], mx[4];
	_mm_store_ps(mn, box.min.quad);
	_mm_store_ps(mx, box.max.quad);
	std::copy(mn, mn + 3, lo);
	std::copy(mx, mx + 3, hi);
}

bool overlaps(const float* alo, const float* ahi, const float* blo, const float* bhi) {
	return alo[0] <= bhi[0] && ahi[0] >= blo[0] && alo[1] <= bhi[1] && ahi[1] >= blo[1] && alo[2] <= bhi[2] && ahi[2] >= blo[2];
}

bool included(RE::COL_LAYER layer) {
	switch (layer) {
		case RE::COL_LAYER::kStatic:
		case RE::COL_LAYER::kAnimStatic:
		case RE::COL_LAYER::kTransparent:
		case RE::COL_LAYER::kTrees:
		case RE::COL_LAYER::kProps:
		case RE::COL_LAYER::kTerrain:
		case RE::COL_LAYER::kGround:
		case RE::COL_LAYER::kInvisibleWall:
		case RE::COL_LAYER::kStairHelper: return true;
		default: return false;
	}
}

// Loose things (baskets, barrels knocked over, bodies) are simulated: the car drives through them.
bool solid(const RE::hkpEntity* entity, bool fixedIsland) {
	if (fixedIsland) return true;
	using M = RE::hkpMotion::MotionType;
	auto type = entity->motion.type.get();
	return type == M::kFixed || type == M::kKeyframed;
}

bool guardedAabb(const RE::hkpShape* shape, const float* xf, RE::hkAabb& out) {
	__try {
		shape->GetAabbImpl(*reinterpret_cast<const RE::hkTransform*>(xf), 0.0f, out);
		return true;
	} __except (EXCEPTION_EXECUTE_HANDLER) {
		return false;
	}
}

struct Body {
	const RE::hkpShape* shape;
	const float* xf;
	bool terrain;
};

struct Collector {
	const Query& q;
	sky::Cache& out;
	Stats& stats;
	float hlo[3], hhi[3];  // Havok world space
	float toSky;
	bool terrain = false;
	std::unordered_set<int>& loggedTypes;

	void toRl(const float* h, float* rl) const { q.frame->toRl({h[0] * toSky, h[1] * toSky, h[2] * toSky}, rl); }
	void dirToRl(const float* h, float* rl) const { space::dirToRl({h[0], h[1], h[2]}, rl); }
	float lenToRl(float h) const { return float(q.frame->lenToRl(h * toSky)); }

	void emitBox(const float* xf, const float* half, float radius) {
		float c[3], ax[3][3], h[3];
		toRl(xf + 12, c);
		for (int i = 0; i < 3; i++) {
			dirToRl(xf + i * 4, ax[i]);
			h[i] = lenToRl(half[i] + radius);
		}
		if (!finite(c, 3) || !finite(h, 3) || h[0] <= 0 || h[1] <= 0 || h[2] <= 0) return;
		out.addBox(c, ax, h);
		stats.boxes++;
	}

	// Shapes we can't read exactly: their bounding box, unless it's huge.
	void emitAabb(const RE::hkpShape* shape, const float* xf) {
		RE::hkAabb box;
		shape->GetAabbImpl(*reinterpret_cast<const RE::hkTransform*>(xf), 0.0f, box);
		float lo[3], hi[3];
		aabbOf(box, lo, hi);
		if (!finite(lo, 3) || !finite(hi, 3) || (hi[0] - lo[0]) * toSky > 5000 || (hi[1] - lo[1]) * toSky > 5000 || (hi[2] - lo[2]) * toSky > 5000)
			return;
		float mid[3] = {(lo[0] + hi[0]) * 0.5f, (lo[1] + hi[1]) * 0.5f, (lo[2] + hi[2]) * 0.5f};
		alignas(16) float frame[16] = {1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, mid[0], mid[1], mid[2], 1};
		float half[3] = {(hi[0] - lo[0]) * 0.5f, (hi[1] - lo[1]) * 0.5f, (hi[2] - lo[2]) * 0.5f};
		emitBox(frame, half, 0);
		stats.fallbacks++;
	}

	void emitTri(const float* a, const float* b, const float* c) {
		float r[3][3];
		const float* w[3] = {a, b, c};
		for (int i = 0; i < 3; i++) toRl(w[i], r[i]);
		if (!finite(r[0], 9)) return;
		if (terrain) {
			// The land is a height field: its outside is up, whatever the winding says.
			float ux = r[1][0] - r[0][0], uy = r[1][1] - r[0][1], vx = r[2][0] - r[0][0], vy = r[2][1] - r[0][1];
			if (ux * vy - uy * vx < 0) std::swap(r[1], r[2]);
		}
		out.addTri(r[0], r[1], r[2], terrain);
	}

	void collect(const RE::hkpShape* shape, const float* xf, int depth) {
		if (!shape || depth > 8 || out.tris.size() > kMaxTris) return;
		using T = RE::hkpShapeType;
		const auto type = shape->type;
		switch (type) {
			case T::kMOPP:
			case T::kBVTree: {
				auto* bv = static_cast<const RE::hkpBvTreeShape*>(shape);
				float llo[3] = {FLT_MAX, FLT_MAX, FLT_MAX}, lhi[3] = {-FLT_MAX, -FLT_MAX, -FLT_MAX};
				for (int c = 0; c < 8; c++) {
					const float p[3] = {(c & 1) ? hhi[0] : hlo[0], (c & 2) ? hhi[1] : hlo[1], (c & 4) ? hhi[2] : hlo[2]};
					const float d[3] = {p[0] - xf[12], p[1] - xf[13], p[2] - xf[14]};
					for (int i = 0; i < 3; i++) {
						const float v = xf[i * 4] * d[0] + xf[i * 4 + 1] * d[1] + xf[i * 4 + 2] * d[2];
						llo[i] = std::min(llo[i], v);
						lhi[i] = std::max(lhi[i], v);
					}
				}
				RE::hkAabb local;
				local.min = RE::hkVector4(llo[0], llo[1], llo[2], 0.0f);
				local.max = RE::hkVector4(lhi[0], lhi[1], lhi[2], 0.0f);
				static thread_local std::vector<RE::hkpShapeKey> keys(kMaxKeys);
				const auto found = std::min<uint32_t>(bv->QueryAabbImpl(local, keys.data(), kMaxKeys), kMaxKeys);
				const auto* container = bv->GetContainer();
				if (!container) return;
				for (uint32_t i = 0; i < found; i++) {
					RE::hkpShapeBuffer buffer;
					collect(container->GetChildShape(keys[i], buffer), xf, depth + 1);
				}
				return;
			}
			case T::kList:
			case T::kCollection:
			case T::kCompressedMesh:
			case T::kExtendedMesh:
			case T::kTriangleCollection:
			case T::kConvexList: {
				const auto* container = shape->GetContainer();
				if (!container) {
					emitAabb(shape, xf);
					return;
				}
				int guard = 0;
				for (auto key = container->GetFirstKey(); key != RE::HK_INVALID_SHAPE_KEY && guard < 200000; key = container->GetNextKey(key), guard++) {
					RE::hkpShapeBuffer buffer;
					const auto* child = container->GetChildShape(key, buffer);
					if (!child) continue;
					RE::hkAabb box;
					child->GetAabbImpl(*reinterpret_cast<const RE::hkTransform*>(xf), 0.0f, box);
					float lo[3], hi[3];
					aabbOf(box, lo, hi);
					if (overlaps(lo, hi, hlo, hhi)) collect(child, xf, depth + 1);
				}
				return;
			}
			case T::kTriangle: {
				float w[3][3];
				for (int v = 0; v < 3; v++) xfPoint(xf, vec(shape, 0x30 + v * 0x10), w[v]);
				if (finite(w[0], 9)) emitTri(w[0], w[1], w[2]);
				return;
			}
			case T::kBox:
				emitBox(xf, vec(shape, 0x30), field<float>(shape, 0x20));
				return;
			case T::kCapsule:
			case T::kSphere: {
				const float zero[3] = {0, 0, 0};
				float wa[3], wb[3], a[3], b[3];
				xfPoint(xf, type == T::kCapsule ? vec(shape, 0x30) : zero, wa);
				xfPoint(xf, type == T::kCapsule ? vec(shape, 0x40) : zero, wb);
				toRl(wa, a);
				toRl(wb, b);
				float r = lenToRl(field<float>(shape, 0x20));
				if (finite(a, 3) && finite(b, 3) && std::isfinite(r) && r > 0 && r < 5000) {
					out.addCapsule(a, b, r);
					stats.capsules++;
				}
				return;
			}
			case T::kConvexVertices: {
				const auto& planes = *reinterpret_cast<const RE::hkArray<RE::hkVector4>*>(reinterpret_cast<const uint8_t*>(shape) + 0x78);
				const float radius = field<float>(shape, 0x20);
				if (planes.size() <= 0 || planes.size() > 512) {
					emitAabb(shape, xf);
					return;
				}
				std::vector<std::array<float, 4>> rl;
				for (int32_t i = 0; i < planes.size(); i++) {
					alignas(16) float p[4];
					_mm_store_ps(p, planes.data()[i].quad);
					float nw[3];
					xfDir(xf, p, nw);
					const float dw = p[3] - (nw[0] * xf[12] + nw[1] * xf[13] + nw[2] * xf[14]) - radius;
					float onPlane[3] = {-nw[0] * dw, -nw[1] * dw, -nw[2] * dw}, n[3], o[3];
					dirToRl(nw, n);
					toRl(onPlane, o);
					if (!finite(n, 3) || !finite(o, 3)) return;
					rl.push_back({n[0], n[1], n[2], -(n[0] * o[0] + n[1] * o[1] + n[2] * o[2])});
				}
				RE::hkAabb box;
				shape->GetAabbImpl(*reinterpret_cast<const RE::hkTransform*>(xf), 0.0f, box);
				float hl[3], hh[3], lo[3] = {FLT_MAX, FLT_MAX, FLT_MAX}, hi[3] = {-FLT_MAX, -FLT_MAX, -FLT_MAX};
				aabbOf(box, hl, hh);
				for (int c = 0; c < 8; c++) {
					float corner[3] = {(c & 1) ? hh[0] : hl[0], (c & 2) ? hh[1] : hl[1], (c & 4) ? hh[2] : hl[2]}, r[3];
					toRl(corner, r);
					for (int i = 0; i < 3; i++) lo[i] = std::min(lo[i], r[i]), hi[i] = std::max(hi[i], r[i]);
				}
				if (!finite(lo, 3) || !finite(hi, 3)) return;
				out.addConvex(rl, lo, hi);
				stats.convexes++;
				return;
			}
			case T::kConvexTransform:
			case T::kConvexTranslate: {
				const auto* child = field<const RE::hkpShape*>(shape, 0x30);
				alignas(16) float local[16] = {1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1};
				if (type == T::kConvexTransform)
					std::memcpy(local, vec(shape, 0x40), sizeof(local));
				else
					std::memcpy(local + 12, vec(shape, 0x40), sizeof(float) * 3);
				if (!child || !xfLooksValid(local)) return;
				alignas(16) float composed[16];
				xfCompose(xf, local, composed);
				collect(child, composed, depth + 1);
				return;
			}
			case T::kTransform: {
				const auto* child = field<const RE::hkpShape*>(shape, 0x28);
				alignas(16) float local[16];
				std::memcpy(local, vec(shape, 0x50), sizeof(local));
				if (!child || !xfLooksValid(local)) return;
				alignas(16) float composed[16];
				xfCompose(xf, local, composed);
				collect(child, composed, depth + 1);
				return;
			}
			default:
				if (loggedTypes.insert(int(type)).second) logger::info("collision: shape type {} read as its bounding box (convex={})", int(type), shape->IsConvex());
				if (shape->IsConvex()) emitAabb(shape, xf);
				return;
		}
	}
};

using CollectFn = void (*)(Collector*, const Body*);
bool guardedCollect(CollectFn fn, Collector* c, const Body* b) {
	__try {
		fn(c, b);
		return true;
	} __except (EXCEPTION_EXECUTE_HANDLER) {
		return false;
	}
}

std::unordered_set<int> g_loggedTypes;

}

bool gather(RE::TESObjectCELL* cell, const Query& q, sky::Cache& out, Stats& stats) {
	auto start = std::chrono::steady_clock::now();
	stats = {};
	out.clear();
	out.setRegion(q.lo, q.hi);
	auto* bhk = cell ? cell->GetbhkWorld() : nullptr;
	auto* world = bhk ? bhk->GetWorld1() : nullptr;
	if (!world) {
		out.build();
		return false;
	}
	const float toHavok = RE::bhkWorld::GetWorldScale();
	Collector col{q, out, stats, {FLT_MAX, FLT_MAX, FLT_MAX}, {-FLT_MAX, -FLT_MAX, -FLT_MAX}, RE::bhkWorld::GetWorldScaleInverse(), false, g_loggedTypes};
	for (int c = 0; c < 8; c++) {
		float p[3] = {(c & 1) ? q.hi[0] : q.lo[0], (c & 2) ? q.hi[1] : q.lo[1], (c & 4) ? q.hi[2] : q.lo[2]};
		space::V3 s = q.frame->toSky(p);
		const double h[3] = {s.x * toHavok, s.y * toHavok, s.z * toHavok};
		for (int i = 0; i < 3; i++) col.hlo[i] = std::min(col.hlo[i], float(h[i])), col.hhi[i] = std::max(col.hhi[i], float(h[i]));
	}

	std::vector<Body> bodies;
	{
		RE::BSReadLockGuard lock(bhk->worldLock);
		auto addIsland = [&](RE::hkpSimulationIsland* island, bool fixed) {
			if (!island) return;
			auto& entities = island->entities;
			for (int32_t i = 0; i < entities.size(); i++) {
				auto* entity = entities.data()[i];
				if (!entity) continue;
				const auto& collidable = entity->collidable;
				const auto layer = collidable.GetCollisionLayer();
				if (!included(layer) || !solid(entity, fixed)) continue;
				const auto* shape = collidable.shape;
				const auto* xf = static_cast<const float*>(collidable.motion);
				if (!shape || !xf || !finite(xf, 16)) continue;
				RE::hkAabb box;
				if (!guardedAabb(shape, xf, box)) {
					stats.faults++;
					continue;
				}
				float lo[3], hi[3];
				aabbOf(box, lo, hi);
				if (!finite(lo, 3) || !finite(hi, 3) || !overlaps(lo, hi, col.hlo, col.hhi)) continue;
				if (q.ignore[0] || q.ignore[1]) {
					auto* ref = RE::TESHavokUtilities::FindCollidableRef(collidable);
					if (ref && (ref == q.ignore[0] || ref == q.ignore[1])) continue;
				}
				bodies.push_back({shape, xf, layer == RE::COL_LAYER::kTerrain || layer == RE::COL_LAYER::kGround});
			}
		};
		addIsland(world->fixedIsland, true);
		for (int32_t i = 0; i < world->activeSimulationIslands.size(); i++) addIsland(world->activeSimulationIslands.data()[i], false);
		for (int32_t i = 0; i < world->inactiveSimulationIslands.size(); i++) addIsland(world->inactiveSimulationIslands.data()[i], false);

		static constexpr CollectFn collect = [](Collector* c, const Body* b) {
			c->terrain = b->terrain;
			c->collect(b->shape, b->xf, 0);
		};
		for (const auto& body : bodies) {
			if (!guardedCollect(collect, &col, &body)) {
				if (stats.faults++ == 0) logger::warn("collision: faulted reading a Havok shape (type {}); skipping it", int(body.shape->type));
			}
		}
	}
	stats.bodies = int(bodies.size());
	stats.tris = int(out.tris.size());
	out.build();
	stats.ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - start).count();
	return true;
}

}

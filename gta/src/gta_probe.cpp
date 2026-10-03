#include "natives.h"
#include "probe_world.h"
#include "space.h"

bool gtaProbe(const space::Frame& frame, int flags, Entity ignore, const float* from, const float* to, ProbeHit& hit) {
	space::V3 a = frame.toGta(from), b = frame.toGta(to);
	int handle = SHAPETEST::START_EXPENSIVE_SYNCHRONOUS_SHAPE_TEST_LOS_PROBE(float(a.x), float(a.y), float(a.z), float(b.x),
	                                                                         float(b.y), float(b.z), flags, ignore, 7);
	BOOL didHit = FALSE;
	Vector3 end{}, normal{};
	Entity entity = 0;
	if (SHAPETEST::GET_SHAPE_TEST_RESULT(handle, &didHit, &end, &normal, &entity) != 2 || !didHit) return false;
	frame.toRl({end.x, end.y, end.z}, hit.point);
	space::dirToRl({normal.x, normal.y, normal.z}, hit.normal);
	hit.entity = entity;
	return true;
}

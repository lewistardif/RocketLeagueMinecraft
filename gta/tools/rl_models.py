#!/usr/bin/env python3
from __future__ import annotations

import argparse
import heapq
import json
import struct
from pathlib import Path

import numpy as np
from PIL import Image

LODS = {
    "body": [6000, 3000, 1500, 800],
    "wheel": [360, 180, 90],
    "ball": [1600, 800, 400],
}


def srgb_to_linear(x):
    return np.where(x <= 0.04045, x / 12.92, ((x + 0.055) / 1.055) ** 2.4)


def linear_to_srgb(x):
    x = np.clip(x, 0.0, 1.0)
    return np.where(x <= 0.0031308, x * 12.92, 1.055 * np.power(x, 1 / 2.4) - 0.055)


def read_accessor(g, buf, idx):
    acc = g["accessors"][idx]
    view = g["bufferViews"][acc["bufferView"]]
    comps = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[acc["type"]]
    dtype = {5126: np.float32, 5125: np.uint32, 5123: np.uint16, 5121: np.uint8}[acc["componentType"]]
    start = view.get("byteOffset", 0) + acc.get("byteOffset", 0)
    stride = view.get("byteStride", 0)
    item = np.dtype(dtype).itemsize * comps
    if stride and stride != item:
        raw = np.frombuffer(buf, np.uint8, count=stride * acc["count"], offset=start).reshape(acc["count"], stride)[:, :item]
        return np.ascontiguousarray(raw).view(dtype).reshape(acc["count"], comps)
    return np.frombuffer(buf, dtype, count=comps * acc["count"], offset=start).reshape(acc["count"], comps)


class Texture:
    def __init__(self, path: Path | None):
        self.img = None
        if path and path.exists():
            self.img = np.asarray(Image.open(path).convert("RGB")).astype(np.float32) / 255.0

    def sample(self, uv):
        if self.img is None:
            return None
        h, w = self.img.shape[:2]
        x = np.mod(uv[:, 0], 1.0) * (w - 1)
        y = np.mod(uv[:, 1], 1.0) * (h - 1)
        return self.img[y.astype(int), x.astype(int)]


def load_gltf(path: Path):
    g = json.loads(path.read_text())
    buf = (path.parent / g["buffers"][0]["uri"]).read_bytes()
    pos, uv, faces, mats = [], [], [], []
    base = 0
    for mesh in g["meshes"]:
        for prim in mesh["primitives"]:
            p = read_accessor(g, buf, prim["attributes"]["POSITION"]).astype(np.float64)
            t = read_accessor(g, buf, prim["attributes"]["TEXCOORD_0"]).astype(np.float64)
            ind = read_accessor(g, buf, prim["indices"]).reshape(-1, 3).astype(np.int64)
            pos.append(p)
            uv.append(t)
            faces.append(ind + base)
            mats.append(np.full(len(ind), prim.get("material", 0)))
            base += len(p)
    return g, np.vstack(pos), np.vstack(uv), np.vstack(faces), np.concatenate(mats)


def material_textures(g, gltf_path: Path, mat_index: int):
    m = g["materials"][mat_index]
    def tex(ref):
        if not ref:
            return Texture(None)
        img = g["images"][g["textures"][ref["index"]]["source"]]["uri"]
        return Texture(gltf_path.parent / img)
    pbr = m.get("pbrMetallicRoughness", {})
    base_factor = np.array(pbr.get("baseColorFactor", [1, 1, 1, 1])[:3], np.float32)
    strength = m.get("extensions", {}).get("KHR_materials_emissive_strength", {}).get("emissiveStrength", 1.0)
    return tex(pbr.get("baseColorTexture")), base_factor, tex(m.get("emissiveTexture")), float(strength)


BARY = np.array([[1 / 3, 1 / 3, 1 / 3], [0.6, 0.2, 0.2], [0.2, 0.6, 0.2], [0.2, 0.2, 0.6], [0.45, 0.45, 0.1], [0.1, 0.45, 0.45], [0.45, 0.1, 0.45]])


def face_colours(g, path, uv, faces, mats):
    lin = np.zeros((len(faces), 3), np.float32)
    glow = np.zeros(len(faces), np.float32)
    for mi in np.unique(mats):
        sel = np.where(mats == mi)[0]
        base, factor, emis, strength = material_textures(g, path, int(mi))
        fuv = uv[faces[sel]]
        acc = np.zeros((len(sel), 3), np.float32)
        eacc = np.zeros((len(sel), 3), np.float32)
        for b in BARY:
            p = (fuv * b[None, :, None]).sum(1)
            s = base.sample(p)
            acc += srgb_to_linear(s) * factor if s is not None else factor
            e = emis.sample(p)
            if e is not None:
                eacc += srgb_to_linear(e)
        acc /= len(BARY)
        eacc /= len(BARY)
        lum = eacc.max(1)
        lit = lum > 0.08
        acc[lit] = np.clip(acc[lit] * 0.3 + eacc[lit] * min(strength, 4.0) * 0.25, 0, 1)
        glow[sel[lit]] = np.clip(lum[lit] * 2.0, 0.3, 1.0)
        lin[sel] = acc
    return lin, glow


def weld(pos, faces, eps=1e-5):
    key = np.round(pos / eps).astype(np.int64)
    _, first, inv = np.unique(key, axis=0, return_index=True, return_inverse=True)
    inv = inv.reshape(-1)
    f = inv[faces]
    keep = (f[:, 0] != f[:, 1]) & (f[:, 1] != f[:, 2]) & (f[:, 0] != f[:, 2])
    return pos[first], f, keep


def plane_quadrics(pos, faces):
    a, b, c = pos[faces[:, 0]], pos[faces[:, 1]], pos[faces[:, 2]]
    n = np.cross(b - a, c - a)
    area = np.linalg.norm(n, axis=1)
    nn = n / np.maximum(area, 1e-20)[:, None]
    d = -(nn * a).sum(1)
    p = np.hstack([nn, d[:, None]])
    q = (p[:, :, None] * p[:, None, :]) * (area[:, None, None] * 0.5 + 1e-12)
    out = np.zeros((len(pos), 4, 4))
    for k in range(3):
        np.add.at(out, faces[:, k], q)
    return out


def decimate(pos, faces, fcol, target):
    pos = pos.copy()
    nv = len(pos)
    faces = [list(f) for f in faces]
    alive = [True] * len(faces)
    vf = [set() for _ in range(nv)]
    for i, f in enumerate(faces):
        for v in f:
            vf[v].add(i)
    Q = list(plane_quadrics(pos, np.array(faces)))
    edges_count = {}
    for f in faces:
        for k in range(3):
            e = (min(f[k], f[(k + 1) % 3]), max(f[k], f[(k + 1) % 3]))
            edges_count[e] = edges_count.get(e, 0) + 1
    for (u, v), cnt in edges_count.items():
        if cnt != 1:
            continue
        fi = next(iter(vf[u] & vf[v]))
        f = faces[fi]
        a, b, c = pos[f[0]], pos[f[1]], pos[f[2]]
        n = np.cross(b - a, c - a)
        e = pos[v] - pos[u]
        bn = np.cross(e, n)
        l = np.linalg.norm(bn)
        if l < 1e-20:
            continue
        bn /= l
        p = np.append(bn, -bn.dot(pos[u]))
        w = np.dot(e, e) * 50.0
        Q[u] = Q[u] + np.outer(p, p) * w
        Q[v] = Q[v] + np.outer(p, p) * w
    vcol = np.zeros((nv, 3))
    vw = np.zeros(nv)
    fa = np.array(faces)
    area = np.linalg.norm(np.cross(pos[fa[:, 1]] - pos[fa[:, 0]], pos[fa[:, 2]] - pos[fa[:, 0]]), axis=1) + 1e-12
    for k in range(3):
        np.add.at(vcol, fa[:, k], fcol * area[:, None])
        np.add.at(vw, fa[:, k], area)
    vcol /= np.maximum(vw, 1e-20)[:, None]
    version = [0] * nv
    scale = float(np.ptp(pos, axis=0).max())

    def cost(u, v):
        q = Q[u] + Q[v]
        a = q[:3, :3]
        b = -q[:3, 3]
        cand = [pos[u], pos[v], (pos[u] + pos[v]) * 0.5]
        try:
            if abs(np.linalg.det(a)) > 1e-12 * scale ** 3:
                x = np.linalg.solve(a, b)
                lo = np.minimum(pos[u], pos[v])
                hi = np.maximum(pos[u], pos[v])
                pad = np.linalg.norm(pos[u] - pos[v]) * 0.1
                if np.all(x >= lo - pad) and np.all(x <= hi + pad):
                    cand.append(x)
        except np.linalg.LinAlgError:
            pass
        best, bp = None, None
        for p in cand:
            h = np.append(p, 1.0)
            c = float(h @ q @ h)
            if best is None or c < best:
                best, bp = c, p
        dc = vcol[u] - vcol[v]
        best += float(dc.dot(dc)) * float(np.dot(pos[u] - pos[v], pos[u] - pos[v])) * 0.05
        return max(best, 0.0), bp

    heap = []
    for (u, v) in edges_count:
        c, p = cost(u, v)
        heapq.heappush(heap, (c, u, v, version[u], version[v]))
    live = len(faces)

    def flips(v, other, newp):
        for fi in vf[v]:
            if not alive[fi]:
                continue
            f = faces[fi]
            if other in f:
                continue
            a, b, c = (pos[x] for x in f)
            n0 = np.cross(b - a, c - a)
            pts = [newp if x == v else pos[x] for x in f]
            n1 = np.cross(pts[1] - pts[0], pts[2] - pts[0])
            l0, l1 = np.linalg.norm(n0), np.linalg.norm(n1)
            if l1 < 1e-14 or l0 < 1e-14:
                return True
            if n0.dot(n1) / (l0 * l1) < 0.3:
                return True
        return False

    while live > target and heap:
        c, u, v, vu, vv = heapq.heappop(heap)
        if version[u] != vu or version[v] != vv:
            continue
        shared = [fi for fi in vf[u] & vf[v] if alive[fi]]
        if not shared:
            continue
        _, p = cost(u, v)
        nu = {x for fi in vf[u] if alive[fi] for x in faces[fi]}
        nvv = {x for fi in vf[v] if alive[fi] for x in faces[fi]}
        if len(nu & nvv) > len(shared) + 2:
            continue
        if flips(u, v, p) or flips(v, u, p):
            continue
        for fi in shared:
            alive[fi] = False
            live -= 1
        for fi in vf[v]:
            if alive[fi]:
                faces[fi] = [u if x == v else x for x in faces[fi]]
                vf[u].add(fi)
        vf[v] = set()
        wu, wv = vw[u], vw[v]
        vcol[u] = (vcol[u] * wu + vcol[v] * wv) / max(wu + wv, 1e-20)
        vw[u] = wu + wv
        pos[u] = p
        Q[u] = Q[u] + Q[v]
        version[u] += 1
        version[v] += 1
        vf[u] = {fi for fi in vf[u] if alive[fi]}
        nbrs = {x for fi in vf[u] for x in faces[fi]} - {u}
        for w in nbrs:
            cc, _ = cost(u, w)
            a, b = (u, w) if u < w else (w, u)
            heapq.heappush(heap, (cc, a, b, version[a], version[b]))
    out = [faces[i] for i in range(len(faces)) if alive[i]]
    used = sorted({x for f in out for x in f})
    remap = {o: n for n, o in enumerate(used)}
    return pos[used], np.array([[remap[x] for x in f] for f in out], np.int64)


def transfer(src_pos, src_faces, src_vals, dst_pos, dst_faces):
    sc = src_pos[src_faces].mean(1)
    sn = np.cross(src_pos[src_faces[:, 1]] - src_pos[src_faces[:, 0]], src_pos[src_faces[:, 2]] - src_pos[src_faces[:, 0]])
    sa = np.linalg.norm(sn, axis=1) + 1e-20
    sn /= sa[:, None]
    a, b, c = dst_pos[dst_faces[:, 0]], dst_pos[dst_faces[:, 1]], dst_pos[dst_faces[:, 2]]
    dn = np.cross(b - a, c - a)
    dl = np.linalg.norm(dn, axis=1) + 1e-20
    dn /= dl[:, None]
    cell = float(np.sqrt(dl.mean())) * 1.5 + 1e-9
    keys = np.floor(sc / cell).astype(np.int64)
    grid = {}
    for i, k in enumerate(map(tuple, keys)):
        grid.setdefault(k, []).append(i)
    out = [np.zeros((len(dst_faces),) + v.shape[1:], v.dtype if v.dtype != np.uint8 else np.float64) for v in src_vals]
    dc = (a + b + c) / 3
    for i in range(len(dst_faces)):
        lo = np.floor((np.minimum(np.minimum(a[i], b[i]), c[i]) - cell) / cell).astype(int)
        hi = np.floor((np.maximum(np.maximum(a[i], b[i]), c[i]) + cell) / cell).astype(int)
        cand = []
        for x in range(lo[0], hi[0] + 1):
            for y in range(lo[1], hi[1] + 1):
                for z in range(lo[2], hi[2] + 1):
                    cand.extend(grid.get((x, y, z), ()))
        cand = np.array(cand, np.int64) if cand else np.zeros(0, np.int64)
        w = None
        if len(cand):
            p = sc[cand]
            v0, v1 = b[i] - a[i], c[i] - a[i]
            d00, d01, d11 = v0.dot(v0), v0.dot(v1), v1.dot(v1)
            v2 = p - a[i]
            d20, d21 = v2 @ v0, v2 @ v1
            den = d00 * d11 - d01 * d01 + 1e-30
            bv = (d11 * d20 - d01 * d21) / den
            bw = (d00 * d21 - d01 * d20) / den
            inside = (bv >= -0.15) & (bw >= -0.15) & (bv + bw <= 1.15)
            facing = (sn[cand] @ dn[i]) > 0.3
            dist = np.abs((p - a[i]) @ dn[i]) < cell
            m = inside & facing & dist
            if m.any():
                cand = cand[m]
                w = sa[cand]
        if w is None:
            d = np.linalg.norm(sc - dc[i], axis=1) - (sn @ dn[i]) * cell
            cand = np.array([int(np.argmin(d))])
            w = np.ones(1)
        for o, v in zip(out, src_vals):
            o[i] = (v[cand] * w.reshape((-1,) + (1,) * (v.ndim - 1))).sum(0) / w.sum()
    return out


def to_rl(p):
    return np.stack([p[:, 0], p[:, 2], p[:, 1]], 1) * 100.0


def write_rlm(path: Path, pos_rl, faces_rl, colour_sets):
    with open(path, "wb") as f:
        f.write(b"RLM1")
        f.write(struct.pack("<III", len(pos_rl), len(faces_rl), len(colour_sets)))
        f.write(pos_rl.astype("<f4").tobytes())
        f.write(faces_rl.astype("<u4").tobytes())
        for cs in colour_sets:
            f.write(cs.astype(np.uint8).tobytes())


def pack_colours(lin, glow):
    rgb = (linear_to_srgb(lin) * 255 + 0.5).astype(np.uint8)
    a = (glow * 255 + 0.5).astype(np.uint8)
    return np.concatenate([rgb, a[:, None]], 1)


def convert(name, gltf_paths, out: Path, lods, post=None):
    meshes = []
    for path in gltf_paths:
        g, pos, uv, faces, mats = load_gltf(path)
        meshes.append((g, path, pos, uv, faces, mats))
    _, path0, pos0, _, faces0, _ = meshes[0]
    wpos, wfaces, keep = weld(pos0, faces0)
    wfaces = wfaces[keep]
    sets = []
    for g, path, pos, uv, faces, mats in meshes:
        lin, glow = face_colours(g, path, uv, faces, mats)
        sets.append((lin[keep], glow[keep]))
    results = []
    for i, target in enumerate(lods):
        if target >= len(wfaces):
            dpos, dfaces = wpos, wfaces
            vals = [v for s in sets for v in s]
        else:
            dpos, dfaces = decimate(wpos, wfaces, sets[0][0], target)
            vals = transfer(wpos, wfaces, [v for s in sets for v in s], dpos, dfaces)
        colour_sets = [pack_colours(vals[2 * k], vals[2 * k + 1]) for k in range(len(sets))]
        prl = to_rl(dpos)
        if post:
            prl = post(prl)
        frl = dfaces[:, [0, 2, 1]]
        write_rlm(out / f"{name}_lod{i}.rlm", prl, frl, colour_sets)
        results.append((len(dfaces), prl, frl, colour_sets))
        print(f"{name} lod{i}: {len(dfaces)} triangles (from {len(wfaces)})")
    return results


def preview(path: Path, items, size=900):
    img = Image.new("RGB", (size * 2, size), (40, 44, 52))
    from PIL import ImageDraw
    dr = ImageDraw.Draw(img)
    light = np.array([0.4, -0.3, 0.85])
    light /= np.linalg.norm(light)
    for panel, (yaw, pitch) in enumerate([(-0.8, 0.35), (2.4, 0.35)]):
        cy, sy, cp, sp = np.cos(yaw), np.sin(yaw), np.cos(pitch), np.sin(pitch)
        polys = []
        for pos, faces, col, offset in items:
            p = pos + offset
            x = p[:, 0] * cy - p[:, 1] * sy
            y = p[:, 0] * sy + p[:, 1] * cy
            z = p[:, 2]
            yy = y
            zz = z * cp - x * sp
            depth = x * cp + z * sp
            a, b, c = p[faces[:, 0]], p[faces[:, 1]], p[faces[:, 2]]
            n = np.cross(b - a, c - a)
            n /= np.linalg.norm(n, axis=1, keepdims=True) + 1e-12
            shade = 0.35 + 0.65 * np.clip(n @ light, 0, 1)
            for i, f in enumerate(faces):
                rgb = col[i, :3].astype(float)
                if col[i, 3] == 0:
                    rgb = rgb * shade[i]
                pts = [(panel * size + size / 2 + yy[k] * 2.6, size / 2 - zz[k] * 2.6) for k in f]
                polys.append((depth[f].mean(), pts, tuple(int(v) for v in np.clip(rgb, 0, 255))))
        polys.sort(key=lambda t: t[0])
        for _, pts, rgb in polys:
            dr.polygon(pts, fill=rgb)
    img.save(path)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--assets", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--preset", default="octane")
    ap.add_argument("--preview", type=Path)
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    car = args.assets / "cars" / args.preset
    body = convert(f"{args.preset}_body", [car / "body_blue.gltf", car / "body_orange.gltf"], args.out, LODS["body"])
    wheel_gltf = args.assets / "wheel" / "wheel.gltf"
    _, wpos, _, _, _ = load_gltf(wheel_gltf)
    wheel_radius = float(np.abs(wpos[:, :2]).max())
    wheel = convert("wheel", [wheel_gltf], args.out, LODS["wheel"], post=lambda p: p / (wheel_radius * 100.0))
    ball_gltf = args.assets / "ball" / "ball.gltf"
    ball = None
    if ball_gltf.exists():
        _, bpos, _, _, _ = load_gltf(ball_gltf)
        ball_radius = float(np.median(np.linalg.norm(bpos, axis=1))) * 100.0
        ball = convert("ball", [ball_gltf], args.out, LODS["ball"], post=lambda p: p / ball_radius)
    anchors = []
    for line in (car / "wheels.txt").read_text().split("\n"):
        parts = line.split()
        if len(parts) == 4:
            x, y, z = (float(v) for v in parts[1:])
            anchors.append(f"{parts[0]} {x * 100:.3f} {z * 100:.3f} {y * 100:.3f}")
    meta = [f"preset {args.preset}", f"lods body {len(LODS['body'])} wheel {len(LODS['wheel'])} ball {len(LODS['ball']) if ball else 0}"]
    (args.out / f"{args.preset}_wheels.txt").write_text("\n".join(meta + anchors) + "\n")
    if args.preview:
        b = body[1]
        items = [(b[1], b[2], b[3][0], np.zeros(3))]
        w = wheel[0]
        for line in anchors:
            name, x, y, z = line.split()
            p = w[1] * 16.0
            if name.endswith("L"):
                p = p * np.array([-1.0, -1.0, 1.0])
            items.append((p, w[2] if not name.endswith("L") else w[2], w[3][0], np.array([float(x), float(y), float(z)])))
        if ball:
            items.append((ball[1][1] * 91.25, ball[1][2], ball[1][3][0], np.array([0.0, 260.0, 60.0])))
        preview(args.preview, items)


if __name__ == "__main__":
    main()

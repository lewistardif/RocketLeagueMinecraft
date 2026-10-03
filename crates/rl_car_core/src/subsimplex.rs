use crate::math::Vec3;

const EQUAL_VERTEX_THRESHOLD: f32 = 0.0001;
const CAST_EPSILON: f32 = 0.0001;
const CAST_MAX_ITERATIONS: u32 = 32;

#[derive(Clone, Copy, Default)]
struct Used {
    a: bool,
    b: bool,
    c: bool,
    d: bool,
}

#[derive(Clone, Copy, Default)]
struct Closest {
    point: Vec3,
    used: Used,
    bary: [f32; 4],
    degenerate: bool,
}

impl Closest {
    fn set(&mut self, a: f32, b: f32, c: f32, d: f32) {
        self.bary = [a, b, c, d];
    }

    fn valid(&self) -> bool {
        self.bary.iter().all(|&x| x >= 0.0)
    }
}

struct Simplex {
    n: usize,
    w: [Vec3; 4],
    p: [Vec3; 4],
    q: [Vec3; 4],
    last_w: Vec3,
    cached_v: Vec3,
    cached_valid: bool,
    needs_update: bool,
    bc: Closest,
}

fn closest_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Closest {
    let mut r = Closest::default();
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        r.point = a;
        r.used.a = true;
        r.set(1.0, 0.0, 0.0, 0.0);
        return r;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        r.point = b;
        r.used.b = true;
        r.set(0.0, 1.0, 0.0, 0.0);
        return r;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        r.point = a + ab * v;
        r.used.a = true;
        r.used.b = true;
        r.set(1.0 - v, v, 0.0, 0.0);
        return r;
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        r.point = c;
        r.used.c = true;
        r.set(0.0, 0.0, 1.0, 0.0);
        return r;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        r.point = a + ac * w;
        r.used.a = true;
        r.used.c = true;
        r.set(1.0 - w, 0.0, w, 0.0);
        return r;
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        r.point = b + (c - b) * w;
        r.used.b = true;
        r.used.c = true;
        r.set(0.0, 1.0 - w, w, 0.0);
        return r;
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    r.point = a + ab * v + ac * w;
    r.used = Used { a: true, b: true, c: true, d: false };
    r.set(1.0 - v - w, v, w, 0.0);
    r
}

fn outside_of_plane(p: Vec3, a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> i32 {
    let normal = (b - a).cross(c - a);
    let sign_p = (p - a).dot(normal);
    let sign_d = (d - a).dot(normal);
    if sign_d * sign_d < 1e-4 * 1e-4 {
        return -1;
    }
    (sign_p * sign_d < 0.0) as i32
}

fn closest_tetrahedron(p: Vec3, a: Vec3, b: Vec3, c: Vec3, d: Vec3, out: &mut Closest) -> bool {
    out.point = p;
    out.used = Used { a: true, b: true, c: true, d: true };
    let abc = outside_of_plane(p, a, b, c, d);
    let acd = outside_of_plane(p, a, c, d, b);
    let adb = outside_of_plane(p, a, d, b, c);
    let bdc = outside_of_plane(p, b, d, c, a);
    if abc < 0 || acd < 0 || adb < 0 || bdc < 0 {
        out.degenerate = true;
        return false;
    }
    if abc == 0 && acd == 0 && adb == 0 && bdc == 0 {
        return false;
    }
    let mut best = f32::MAX;
    if abc != 0 {
        let t = closest_triangle(p, a, b, c);
        let sq = (t.point - p).dot(t.point - p);
        if sq < best {
            best = sq;
            out.point = t.point;
            out.used = Used { a: t.used.a, b: t.used.b, c: t.used.c, d: false };
            out.set(t.bary[0], t.bary[1], t.bary[2], 0.0);
        }
    }
    if acd != 0 {
        let t = closest_triangle(p, a, c, d);
        let sq = (t.point - p).dot(t.point - p);
        if sq < best {
            best = sq;
            out.point = t.point;
            out.used = Used { a: t.used.a, b: false, c: t.used.b, d: t.used.c };
            out.set(t.bary[0], 0.0, t.bary[1], t.bary[2]);
        }
    }
    if adb != 0 {
        let t = closest_triangle(p, a, d, b);
        let sq = (t.point - p).dot(t.point - p);
        if sq < best {
            best = sq;
            out.point = t.point;
            out.used = Used { a: t.used.a, b: t.used.c, c: false, d: t.used.b };
            out.set(t.bary[0], t.bary[2], 0.0, t.bary[1]);
        }
    }
    if bdc != 0 {
        let t = closest_triangle(p, b, d, c);
        let sq = (t.point - p).dot(t.point - p);
        if sq < best {
            out.point = t.point;
            out.used = Used { a: false, b: t.used.a, c: t.used.c, d: t.used.b };
            out.set(0.0, t.bary[0], t.bary[2], t.bary[1]);
        }
    }
    true
}

impl Simplex {
    fn new() -> Simplex {
        Simplex {
            n: 0,
            w: [Vec3::ZERO; 4],
            p: [Vec3::ZERO; 4],
            q: [Vec3::ZERO; 4],
            last_w: Vec3::new(1e18, 1e18, 1e18),
            cached_v: Vec3::ZERO,
            cached_valid: false,
            needs_update: true,
            bc: Closest::default(),
        }
    }

    fn remove(&mut self, i: usize) {
        self.n -= 1;
        self.w[i] = self.w[self.n];
        self.p[i] = self.p[self.n];
        self.q[i] = self.q[self.n];
    }

    fn reduce(&mut self, u: Used) {
        if self.n >= 4 && !u.d {
            self.remove(3);
        }
        if self.n >= 3 && !u.c {
            self.remove(2);
        }
        if self.n >= 2 && !u.b {
            self.remove(1);
        }
        if self.n >= 1 && !u.a {
            self.remove(0);
        }
    }

    fn add(&mut self, w: Vec3, p: Vec3, q: Vec3) {
        self.last_w = w;
        self.needs_update = true;
        self.w[self.n] = w;
        self.p[self.n] = p;
        self.q[self.n] = q;
        self.n += 1;
    }

    fn contains(&self, w: Vec3) -> bool {
        let d2 = |a: Vec3| (a - w).length_squared();
        self.w[..self.n].iter().any(|&x| d2(x) <= EQUAL_VERTEX_THRESHOLD) || w == self.last_w
    }

    fn weighted(points: &[Vec3; 4], bary: &[f32; 4], n: usize) -> Vec3 {
        let mut s = points[0] * bary[0];
        for i in 1..n {
            s += points[i] * bary[i];
        }
        s
    }

    fn update(&mut self) -> bool {
        if !self.needs_update {
            return self.cached_valid;
        }
        self.bc = Closest::default();
        self.needs_update = false;
        match self.n {
            1 => {
                self.cached_v = self.p[0] - self.q[0];
                self.bc.set(1.0, 0.0, 0.0, 0.0);
                self.cached_valid = self.bc.valid();
            }
            2 => {
                let from = self.w[0];
                let v = self.w[1] - from;
                let diff = Vec3::ZERO - from;
                let mut t = v.dot(diff);
                if t > 0.0 {
                    let vv = v.dot(v);
                    if t < vv {
                        t /= vv;
                        self.bc.used.a = true;
                        self.bc.used.b = true;
                    } else {
                        t = 1.0;
                        self.bc.used.b = true;
                    }
                } else {
                    t = 0.0;
                    self.bc.used.a = true;
                }
                self.bc.set(1.0 - t, t, 0.0, 0.0);
                let p1 = self.p[0] + (self.p[1] - self.p[0]) * t;
                let p2 = self.q[0] + (self.q[1] - self.q[0]) * t;
                self.cached_v = p1 - p2;
                self.reduce(self.bc.used);
                self.cached_valid = self.bc.valid();
            }
            3 => {
                self.bc = closest_triangle(Vec3::ZERO, self.w[0], self.w[1], self.w[2]);
                let p1 = Self::weighted(&self.p, &self.bc.bary, 3);
                let p2 = Self::weighted(&self.q, &self.bc.bary, 3);
                self.cached_v = p1 - p2;
                self.reduce(self.bc.used);
                self.cached_valid = self.bc.valid();
            }
            4 => {
                let mut bc = Closest::default();
                let sep = closest_tetrahedron(Vec3::ZERO, self.w[0], self.w[1], self.w[2], self.w[3], &mut bc);
                self.bc = bc;
                if sep {
                    let p1 = Self::weighted(&self.p, &self.bc.bary, 4);
                    let p2 = Self::weighted(&self.q, &self.bc.bary, 4);
                    self.cached_v = p1 - p2;
                    self.reduce(self.bc.used);
                    self.cached_valid = self.bc.valid();
                } else if self.bc.degenerate {
                    self.cached_valid = false;
                } else {
                    self.cached_valid = true;
                    self.cached_v = Vec3::ZERO;
                }
            }
            _ => self.cached_valid = false,
        }
        self.cached_valid
    }
}

fn sphere_support(center: Vec3, radius: f32, dir: Vec3) -> Vec3 {
    let n = if dir.length_squared() < f32::EPSILON * f32::EPSILON { Vec3::new(-1.0, -1.0, -1.0).normalized() } else { dir.normalized() };
    center + n * radius
}

pub(crate) fn ray_sphere(from: Vec3, to: Vec3, center: Vec3, radius: f32) -> Option<(f32, Vec3)> {
    let mut simplex = Simplex::new();
    let r = to - from;
    let mut lambda = 0.0f32;
    let mut pos_a = from;
    let mut v = pos_a - sphere_support(center, radius, r);
    let mut n = Vec3::ZERO;
    let mut dist2 = v.length_squared();
    let mut iterations = CAST_MAX_ITERATIONS;
    while dist2 > CAST_EPSILON && iterations > 0 {
        iterations -= 1;
        let sup_a = pos_a;
        let sup_b = sphere_support(center, radius, v);
        let w = sup_a - sup_b;
        let v_dot_w = v.dot(w);
        if lambda > 1.0 {
            return None;
        }
        if v_dot_w > 0.0 {
            let v_dot_r = v.dot(r);
            if v_dot_r >= -(f32::EPSILON * f32::EPSILON) {
                return None;
            }
            lambda -= v_dot_w / v_dot_r;
            pos_a = from * (1.0 - lambda) + to * lambda;
            n = v;
        }
        if !simplex.contains(w) {
            simplex.add(w, sup_a, sup_b);
        }
        if simplex.update() {
            v = simplex.cached_v;
            dist2 = v.length_squared();
        } else {
            v = simplex.cached_v;
            dist2 = 0.0;
        }
    }
    let normal = if n.length_squared() >= f32::EPSILON * f32::EPSILON { n.normalized() } else { Vec3::ZERO };
    if normal.dot(r) >= 0.0 {
        return None;
    }
    if normal.length_squared() <= 0.0001 || lambda >= 1.0 {
        return None;
    }
    Some((lambda, normal.normalized()))
}

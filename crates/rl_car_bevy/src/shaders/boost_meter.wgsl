// Rocket League's boost meter, one layer of it (a Scaleform clip; see `hud.rs`).
//
// Vertices are in the meter clip's space (movie px, y down, z towards the screen). The vertex
// stage tilts and projects them like Flash 3D does (`BoostMeterLayout::project` in rl_car_core),
// writing clip space with w = the perspective divisor so textures are perspective correct.
// The fragment stage applies the layer's Flash colour transform. The target is the meter's own
// float buffer, which keeps gamma-space values so layers blend like Scaleform's (straight alpha
// over a transparent clear: the buffer ends up premultiplied).
//
// Bitmap layers sample their texture (straight alpha). Text layers sample a glyph atlas: R is the
// glyph coverage, G / B the coverage blurred for the text's GlowFilter (blur 6 / 8). The vertex
// colour's red picks the glyph (1) or its glow (0); the glow is black before the transform.

struct Meter {
    rotation_x: vec4<f32>,   // rows of the clip's rotation (rotationX then rotationY)
    rotation_y: vec4<f32>,
    rotation_z: vec4<f32>,
    position: vec4<f32>,     // meter origin (movie px), screen px per movie px, focal length
    center: vec4<f32>,       // projection centre (movie px), viewport size (px)
    mult: vec4<f32>,         // colour transform
    add: vec4<f32>,          // its offsets / 255
    text_color: vec4<f32>,   // text: colour (0..1); w = 1 for text layers
    params: vec4<f32>,       // x: layer scale about the origin, y: glow channel (1 = G, 2 = B)
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> meter: Meter;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var layer_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var layer_sampler: sampler;

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(4) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) glyph: f32,
}

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    let p = vec3(v.position.xy * meter.params.x, v.position.z);
    let r = vec3(dot(meter.rotation_x.xyz, p), dot(meter.rotation_y.xyz, p), dot(meter.rotation_z.xyz, p));
    let at = meter.position.xy + r.xy;
    let focal = meter.position.w;
    let w = (focal + r.z) / focal;
    let screen = (meter.center.xy + (at - meter.center.xy) / w) * meter.position.z;
    let ndc = vec2(screen.x / meter.center.z * 2.0 - 1.0, 1.0 - screen.y / meter.center.w * 2.0);
    var out: VertexOutput;
    out.clip = vec4(ndc * w, 0.0, w);
    out.uv = v.uv;
    out.glyph = v.color.r;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let s = textureSample(layer_texture, layer_sampler, in.uv);
    var rgb: vec3<f32>;
    var a: f32;
    if meter.text_color.w > 0.5 {
        let glow = select(s.b, s.g, meter.params.y < 1.5);
        let is_glyph = in.glyph > 0.5;
        rgb = select(vec3(0.0), meter.text_color.rgb, is_glyph);
        a = select(glow, s.r, is_glyph);
    } else {
        rgb = s.rgb;
        a = s.a;
    }
    rgb = clamp(rgb * meter.mult.rgb + meter.add.rgb, vec3(0.0), vec3(1.0));
    a = clamp(a * meter.mult.a + meter.add.a, 0.0, 1.0);
    return vec4(rgb, a);
}

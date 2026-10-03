// Rocket League's default boost flame: material `BoostMesh_Paintable_MAT` (instance
// `MasterBoost_Standard_MIC`), translated line by line from the game's compiled pixel shader
// (RefShaderCache, base pass, no light map). Additive, unlit, two-sided.
//
// Inputs as the game feeds them: UV0, UV1.x (> 0.5 marks the inner shell of the cone), and the
// cosine between the surface normal and the direction to the camera. Texture 0 is
// `Water_02_N` (a scrolling distortion), texture 1 `ParticleSheet_T` (the sparks).

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::{view, globals}

struct FlameParams {
    // CustomColor.rgb * Brightness
    color: vec4<f32>,
    // Inner_Speed, Outer_Speed, TileX, TileY
    speed_tile: vec4<f32>,
    // Inner_Sparks, Outer_Sparks, GradientAmount, GradientSharpness
    sparks_gradient: vec4<f32>,
    // FresnelBase, FresnelEnd, Opacity, unused
    fresnel_opacity: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> p: FlameParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var noise_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var noise_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var sparks_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var sparks_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
#ifdef VERTEX_UVS_B
    let uv1x = in.uv_b.x;
#else
    let uv1x = in.uv.x;
#endif
    let ndv = dot(normalize(in.world_normal), normalize(view.world_position - in.world_position.xyz));
    let time = globals.time;

    let inner = saturate((uv1x - 0.5) * 100.0);
    let t = mix(p.speed_tile.y, p.speed_tile.x, inner) * time;

    // Two scrolling samples of the distortion texture.
    let n1 = textureSample(noise_texture, noise_sampler, uv * vec2(1.0, 0.25) + vec2(-0.05 * t, t)).xy;
    let n2 = textureSample(noise_texture, noise_sampler, uv * vec2(1.0, 0.5) + vec2(0.02 * t, t)).xy;
    let n = n1 + n2;

    // Sparks: two scrolling, distorted samples multiplied, minus a threshold.
    let tiled = vec2(uv.x * p.speed_tile.z, uv.y * p.speed_tile.w * 0.2);
    let s1 = textureSample(sparks_texture, sparks_sampler, tiled + vec2(0.0, 2.0 * t) + 0.1 * n).x;
    let s2 = textureSample(sparks_texture, sparks_sampler, tiled + vec2(0.0, t) + 0.1 * n).x;
    var sparks = saturate(s2 * s1 - mix(p.sparks_gradient.y, p.sparks_gradient.x, inner));

    // Facing ratio raised to an exponent that goes from FresnelEnd at the nozzle to FresnelBase.
    let v = abs(uv.y);
    var exponent_blend = min(v * v * v * v * 4.0, 1.0) * (p.fresnel_opacity.x - p.fresnel_opacity.y);
    if v < 0.000001 {
        exponent_blend = 0.0;
    }
    var facing = min(pow(abs(ndv), exponent_blend + p.fresnel_opacity.y), 1.0);

    // Length gradient, along the distorted V.
    let along = n.y * 0.5 + uv.y;
    facing *= saturate(pow(abs(along), p.sparks_gradient.z) * p.sparks_gradient.w);
    if abs(along) < 0.000001 || abs(ndv) < 0.000001 {
        facing = 0.0;
    }
    let tail = saturate(1.0 - along);
    let tail4 = tail * tail * tail * tail;
    sparks *= facing * saturate(ndv) * min(tail4 * 64.0, 1.0) * 64.0;
    if tail < 0.000001 {
        sparks = 0.0;
    }

    // Colour: sqrt(colour) near the nozzle towards the colour itself further back; the inner
    // shell is the colour brightened by 0.1.
    let c = p.color.rgb;
    var base = min(sqrt(c), vec3(1.0));
    base = mix(base, c, saturate(uv.y - 0.5) * 3.0);
    base = mix(base, saturate(c + 0.1), inner);
    let rgb = sparks * c + base;
    let alpha = saturate((sparks + facing) * p.fresnel_opacity.z);
    // Additive: the game writes alpha * rgb and blends One/One.
    return vec4(alpha * rgb, 0.0);
}

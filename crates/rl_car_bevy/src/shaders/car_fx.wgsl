// Rocket League's car particle materials, translated from the game's compiled pixel shaders
// (RefShaderCache, base pass, particle sprite / sub-UV / beam-trail vertex factories; found and
// disassembled with tools/rl_assets/shader_cache.py). Unlit; fog is off (its colour 0, factor 1),
// and the editor-only SelectionColor uniform is 0, so neither appears below.
//
// `kind` picks the material. Inputs as the game's vertex factories pass them: UV is the sprite's
// texture coordinate (its sub-UV cell for sub-UV sprites); for ribbons UV is (along the trail 0 at
// the head .. 1 at the tail, across 0..1) and UV_B is (across 0..1, distance along the trail /
// TilingDistance). The vertex colour is the particle colour (HDR) and alpha.
//
// Additive materials write rgb with alpha 0 (drawn One/One); translucent ones write straight alpha.

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::globals

struct Params {
    kind: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: Params;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var tex_sampler: sampler;

// HLSL frc on a signed value as the shaders compute it: x - trunc(x).
fn signed_frac(x: f32) -> f32 {
    return select(-fract(abs(x)), fract(abs(x)), x >= -x);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
#ifdef VERTEX_UVS_A
    let uv = in.uv;
#else
    let uv = vec2(0.0);
#endif
#ifdef VERTEX_UVS_B
    let uv_b = in.uv_b;
#else
    let uv_b = vec2(0.0);
#endif
#ifdef VERTEX_COLORS
    let c = in.color;
#else
    let c = vec4(1.0);
#endif
    let time = globals.time;

    switch params.kind {
        // SupersonicStreaks_Mat (additive): (R + B) / 2 of Radial_Generic_01_Pack. The particle
        // colour is not used.
        case 1u: {
            let t = textureSample(tex, tex_sampler, uv);
            return vec4(vec3((t.r + t.b) * 0.5), 0.0);
        }
        // Smoke_Puff_01_Mat (translucent): a panning noise (R) distorts the lookup of the puff mask
        // (B); the colour is the particle colour times a squared ramp down each sub-image.
        case 2u: {
            let pan = vec2(fract(time * -0.093802), fract(time * 0.1125));
            let n = textureSample(tex, tex_sampler, uv + pan).r;
            let mask = textureSample(tex, tex_sampler, uv + n * 0.1).b;
            let ramp = 1.0 - signed_frac((n - 0.125) * 0.5 + uv.y * 2.0);
            return vec4(ramp * ramp * c.rgb, mask * c.a);
        }
        // Glow01_MIC (Unlit_Translucent_Mat permutation; writes alpha 0, so it adds): GradientCircle01
        // times the particle colour, weighted by its red channel times the particle alpha
        // (MeshEmitterVertexColor is its default, 1).
        case 3u: {
            let t = textureSample(tex, tex_sampler, uv).rgb;
            let w = t.r * c.a;
            return vec4(t * c.rgb * w, 0.0);
        }
        // Spark_Mat (translucent, no texture): a soft-edged quad, 3x the distance to each edge.
        case 4u: {
            let e = (1.0 - abs(uv * 2.0 - 1.0)) * 3.0;
            return vec4(c.rgb, saturate(e.x * e.y) * c.a);
        }
        // Glow_Translucent_Mat (translucent): the particle colour, alpha = texture red x particle alpha.
        case 5u: {
            let t = textureSample(tex, tex_sampler, uv).r;
            return vec4(c.rgb, t * c.a);
        }
        // Wheel_Trail_Mat (additive ribbon): two panning lookups of Noise_Fire_02_Pack (G), a ridge
        // across the trail, faded towards the tail and in over the first 1/18 tile at the head.
        case 6u: {
            let ridge = 1.0 - abs(uv_b.x * 2.0 - 1.0);
            let y = ridge * 0.1875 + uv_b.y;
            let a = ridge * c.a;
            let pan1 = vec2(fract(time * 0.033333), fract(time * -0.1075));
            let pan2 = vec2(fract(time * -0.0109), fract(time * -0.030315));
            let w = textureSample(tex, tex_sampler, vec2(uv_b.x * 0.5, y) + pan2).g;
            let r = textureSample(tex, tex_sampler, vec2(uv_b.x, y * 2.0) + pan1 + w * 0.125).g;
            let core = saturate((a * a - 0.875) * 4.0);
            var q = saturate(a * 1.25 + (w + r - 1.0)) + core;
            q = min(q, 1.0) * (1.0 - uv.x) * saturate(uv_b.y * 18.0);
            return vec4(q * (vec3(core) + c.rgb), 0.0);
        }
        // DodgeRibbon_Mat (additive ribbon, no texture): a bright core across the ribbon, faded at
        // both ends, whitened towards the centre.
        case 7u: {
            let across = 1.0 - min(abs(1.0 - uv.y * 2.0) * 2.0, 1.0);
            let profile = across * across + 0.25;
            let ends = 1.0 - saturate((1.0 - abs(1.0 - uv.x * 2.0)) * 4.0);
            let d = vec2(ends, 1.0 - uv.y * 2.0);
            let round = max(1.0 - dot(d, d), 0.0);
            let k = round * round * profile;
            let rgb = k * (c.rgb - 1.0) + 1.0;
            return vec4(rgb * k * c.a, 0.0);
        }
        // StandardFlare_Mat (additive): the flare texture times the particle colour and alpha.
        case 8u: {
            let t = textureSample(tex, tex_sampler, uv).rgb;
            return vec4(t * c.rgb * c.a, 0.0);
        }
        default: {
            return vec4(0.0);
        }
    }
}

#version 330
#extension GL_ARB_separate_shader_objects : require

// Rocket League's car particle materials, translated from the game's compiled pixel shaders. Same
// as the Bevy demo's crates/rl_car_bevy/src/shaders/car_fx.wgsl, one material per pipeline:
// FX_KIND picks it, ADDITIVE says how it blends. Unlit. Inputs as the game's vertex factories pass
// them: texCoord0 is the sprite's texture coordinate (its sub-UV cell for sub-UV sprites); for
// ribbons texCoord0 is (along the trail, 0 at the head .. 1 at the tail, across 0..1) and texCoord1
// (across 0..1, distance along the trail / TilingDistance). The vertex colour is the particle
// colour (HDR) and alpha. Sampler0 is the material's texture, sampled as stored (not sRGB).
//
// The game adds or blends linear light into an HDR scene colour. With FX_LINEAR (the usual way,
// dev.rlcar.client.LinearFx) the output is just that: linear premultiplied light for an RGBA16F
// target blended One / OneMinusSrcAlpha (additive: rgb, alpha 0; translucent: rgb * alpha, alpha),
// composited over the scene once. Without it (shader packs, or the pass unavailable) Minecraft
// blends sRGB values into its main target, so the result is converted to sRGB first, which makes
// faint additive glows much brighter than in the game.

#include <minecraft:globals.glsl>
#include <minecraft:fog.glsl>
#include <minecraft:dynamictransforms.glsl>
#include <minecraft:oit.glsl>

uniform sampler2D Sampler0;

layout(location = 0) in float sphericalVertexDistance;
layout(location = 1) in float cylindricalVertexDistance;
layout(location = 2) in vec2 texCoord0;
layout(location = 3) in vec2 texCoord1;
layout(location = 4) in vec4 vertexColor;

#ifndef OIT_ALPHA_ONLY
layout(location = 0) out vec4 fragColor;
#endif

// HLSL frc on a signed value as the shaders compute it: x - trunc(x).
float signedFrac(float x) {
    return x >= -x ? fract(abs(x)) : -fract(abs(x));
}

vec3 linearToSrgb(vec3 c) {
    c = max(c, vec3(0.0));
    return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, step(vec3(0.0031308), c));
}

vec3 srgbToLinear(vec3 c) {
    return mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), c));
}

// The texture as an anisotropic sampler reads it (4x: the game's MaxAnisotropy, TASystemSettings.ini):
// the mip level from the footprint's short axis, taps spread along its long axis. The supersonic streaks are a radial blob
// stretched along the velocity to ~1 pixel by hundreds: a trilinear lookup takes its mip level from
// the long axis and turns every streak into a flat line of the texture's average.
vec4 textureAniso(vec2 uv) {
    vec2 dx = dFdx(uv);
    vec2 dy = dFdy(uv);
    float lx = length(dx);
    float ly = length(dy);
    vec2 major = lx > ly ? dx : dy;
    float lMajor = max(lx, ly);
    float lMinor = max(min(lx, ly), 1e-8);
    float n = clamp(ceil(lMajor / lMinor), 1.0, 4.0);
    vec2 size = vec2(textureSize(Sampler0, 0));
    float lod = log2(max(lMajor / n, lMinor) * max(size.x, size.y));
    vec4 sum = vec4(0.0);
    for (float i = 0.0; i < n; i += 1.0) {
        sum += textureLod(Sampler0, uv + major * ((i + 0.5) / n - 0.5), lod);
    }
    return sum / n;
}

// The material: linear rgb and (translucent materials) alpha.
vec4 shade(vec2 uv, vec2 uvB, vec4 c) {
    float time = GameTime * 1200.0; // seconds
#if FX_KIND == 1
    // SupersonicStreaks_Mat (additive): (R + B) / 2 of Radial_Generic_01_Pack. The particle colour
    // is not used (the compiled shader has no vertex colour input).
    vec4 t = textureAniso(uv);
    return vec4(vec3((t.r + t.b) * 0.5), 0.0);
#elif FX_KIND == 2
    // Smoke_Puff_01_Mat (translucent): a panning noise (R) distorts the lookup of the puff mask
    // (B); the colour is the particle colour times a squared ramp down each sub-image.
    vec2 pan = vec2(fract(time * -0.093802), fract(time * 0.1125));
    float n = texture(Sampler0, uv + pan).r;
    float mask = texture(Sampler0, uv + n * 0.1).b;
    float ramp = 1.0 - signedFrac((n - 0.125) * 0.5 + uv.y * 2.0);
    return vec4(ramp * ramp * c.rgb, mask * c.a);
#elif FX_KIND == 3
    // Glow01_MIC (Unlit_Translucent_Mat permutation; writes alpha 0, so it adds): GradientCircle01
    // times the particle colour, weighted by its red channel times the particle alpha.
    vec3 t = texture(Sampler0, uv).rgb;
    float w = t.r * c.a;
    return vec4(t * c.rgb * w, 0.0);
#elif FX_KIND == 4
    // Spark_Mat (translucent, no texture): a soft-edged quad, 3x the distance to each edge.
    vec2 e = (1.0 - abs(uv * 2.0 - 1.0)) * 3.0;
    return vec4(c.rgb, clamp(e.x * e.y, 0.0, 1.0) * c.a);
#elif FX_KIND == 5
    // Glow_Translucent_Mat (translucent): the particle colour, alpha = texture red x particle alpha.
    float t = texture(Sampler0, uv).r;
    return vec4(c.rgb, t * c.a);
#elif FX_KIND == 6
    // Wheel_Trail_Mat (additive ribbon): two panning lookups of Noise_Fire_02_Pack (G), a ridge
    // across the trail, faded towards the tail and in over the first 1/18 tile at the head.
    float ridge = 1.0 - abs(uvB.x * 2.0 - 1.0);
    float y = ridge * 0.1875 + uvB.y;
    float a = ridge * c.a;
    vec2 pan1 = vec2(fract(time * 0.033333), fract(time * -0.1075));
    vec2 pan2 = vec2(fract(time * -0.0109), fract(time * -0.030315));
    float w = texture(Sampler0, vec2(uvB.x * 0.5, y) + pan2).g;
    float r = texture(Sampler0, vec2(uvB.x, y * 2.0) + pan1 + w * 0.125).g;
    float core = clamp((a * a - 0.875) * 4.0, 0.0, 1.0);
    float q = clamp(a * 1.25 + (w + r - 1.0), 0.0, 1.0) + core;
    q = min(q, 1.0) * (1.0 - uv.x) * clamp(uvB.y * 18.0, 0.0, 1.0);
    return vec4(q * (vec3(core) + c.rgb), 0.0);
#elif FX_KIND == 7
    // DodgeRibbon_Mat (additive ribbon, no texture): a bright core across the ribbon, faded at
    // both ends, whitened towards the centre.
    float across = 1.0 - min(abs(1.0 - uv.y * 2.0) * 2.0, 1.0);
    float profile = across * across + 0.25;
    float ends = 1.0 - clamp((1.0 - abs(1.0 - uv.x * 2.0)) * 4.0, 0.0, 1.0);
    vec2 d = vec2(ends, 1.0 - uv.y * 2.0);
    float round = max(1.0 - dot(d, d), 0.0);
    float k = round * round * profile;
    vec3 rgb = k * (c.rgb - 1.0) + 1.0;
    return vec4(rgb * k * c.a, 0.0);
#elif FX_KIND == 8
    // StandardFlare_Mat (additive): the flare texture times the particle colour and alpha.
    vec3 t = texture(Sampler0, uv).rgb;
    return vec4(t * c.rgb * c.a, 0.0);
#else
    return vec4(0.0);
#endif
}

void main() {
    vec4 s = shade(texCoord0, texCoord1, vertexColor);
    float fog = total_fog_value(sphericalVertexDistance, cylindricalVertexDistance, FogEnvironmentalStart, FogEnvironmentalEnd, FogRenderDistanceStart, FogRenderDistanceEnd);
#if defined(FX_LINEAR) && defined(ADDITIVE)
    // Faded out in the fog, as below.
    fragColor = vec4(max(s.rgb, 0.0) * ColorModulator.rgb * (1.0 - fog), 0.0);
#elif defined(FX_LINEAR)
    // Towards the fog colour (apply_fog, in linear light), then premultiplied.
    float a = clamp(s.a, 0.0, 1.0) * ColorModulator.a;
    vec3 rgb = mix(max(s.rgb, 0.0) * ColorModulator.rgb, srgbToLinear(FogColor.rgb), fog * FogColor.a);
    fragColor = vec4(rgb * a, a);
#elif defined(ADDITIVE)
    vec3 added = clamp(s.rgb, 0.0, 1.0) * (1.0 - fog);
    vec4 color = vec4(linearToSrgb(added), 1.0) * ColorModulator;
    #ifdef OIT_ALPHA_ONLY
    executeAlphaOnlyPhase(gl_FragCoord.z, 0.0);
    #elif defined(OIT_ACCUMULATE)
    fragColor = sampleColorForAccumulation(color);
    #else
    fragColor = color;
    #endif
#else
    vec4 color = vec4(linearToSrgb(clamp(s.rgb, 0.0, 1.0)), clamp(s.a, 0.0, 1.0)) * ColorModulator;
    #ifdef OIT_ALPHA_ONLY
    executeAlphaOnlyPhase(gl_FragCoord.z, color.a);
    #else
    #ifdef OIT_ACCUMULATE
    color = sampleColorForAccumulation(color);
    vec4 fogColor = vec4(FogColor.rgb * color.a, FogColor.a);
    #else
    vec4 fogColor = FogColor;
    #endif
    fragColor = apply_fog(color, sphericalVertexDistance, cylindricalVertexDistance, FogEnvironmentalStart, FogEnvironmentalEnd, FogRenderDistanceStart, FogRenderDistanceEnd, fogColor);
    #endif
#endif
}

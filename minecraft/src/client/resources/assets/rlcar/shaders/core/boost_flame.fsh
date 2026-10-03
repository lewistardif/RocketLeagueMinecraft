#version 330
#extension GL_ARB_separate_shader_objects : require

// Rocket League's default boost flame: material BoostMesh_Paintable_MAT (instance
// MasterBoost_Standard_MIC), translated from the game's compiled pixel shader. Same as the Bevy
// demo's crates/rl_car_bevy/src/shaders/boost_flame.wgsl. Additive, unlit, two-sided.
// Sampler0: Water_02_N (scrolling distortion), Sampler1: ParticleSheet_T (sparks), both sRGB.
// The material parameters come in as defines (from the extracted boost.json):
// COLOR_R/G/B (CustomColor * Brightness), INNER_SPEED, OUTER_SPEED, TILE_X, TILE_Y,
// INNER_SPARKS, OUTER_SPARKS, GRADIENT_AMOUNT, GRADIENT_SHARPNESS, FRESNEL_BASE, FRESNEL_END, OPACITY.

#include <minecraft:globals.glsl>
#include <minecraft:fog.glsl>
#include <minecraft:dynamictransforms.glsl>
#include <minecraft:oit.glsl>

uniform sampler2D Sampler0;
uniform sampler2D Sampler1;

layout(location = 0) in float sphericalVertexDistance;
layout(location = 1) in float cylindricalVertexDistance;
layout(location = 2) in vec2 texCoord0;
layout(location = 3) in float texCoord1U;
layout(location = 4) in vec3 normal;
layout(location = 5) in vec3 toCamera;

#ifndef OIT_ALPHA_ONLY
layout(location = 0) out vec4 fragColor;
#endif

vec2 sampleLinear(sampler2D s, vec2 uv) {
    vec2 c = texture(s, uv).xy;
    return mix(c / 12.92, pow((c + 0.055) / 1.055, vec2(2.4)), step(vec2(0.04045), c));
}

float linearToSrgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

void main() {
    vec2 uv = texCoord0;
    float ndv = dot(normalize(normal), normalize(toCamera));
    float time = GameTime * 1200.0; // seconds (the day cycle wraps, the scroll speeds wrap with it)

    float inner = clamp((texCoord1U - 0.5) * 100.0, 0.0, 1.0);
    float t = mix(OUTER_SPEED, INNER_SPEED, inner) * time;

    vec2 n = sampleLinear(Sampler0, uv * vec2(1.0, 0.25) + vec2(-0.05 * t, t))
        + sampleLinear(Sampler0, uv * vec2(1.0, 0.5) + vec2(0.02 * t, t));

    vec2 tiled = vec2(uv.x * TILE_X, uv.y * TILE_Y * 0.2);
    float s1 = sampleLinear(Sampler1, tiled + vec2(0.0, 2.0 * t) + 0.1 * n).x;
    float s2 = sampleLinear(Sampler1, tiled + vec2(0.0, t) + 0.1 * n).x;
    float sparks = clamp(s2 * s1 - mix(OUTER_SPARKS, INNER_SPARKS, inner), 0.0, 1.0);

    float v = abs(uv.y);
    float exponentBlend = v < 0.000001 ? 0.0 : min(v * v * v * v * 4.0, 1.0) * (FRESNEL_BASE - FRESNEL_END);
    float facing = min(pow(abs(ndv), exponentBlend + FRESNEL_END), 1.0);

    float along = n.y * 0.5 + uv.y;
    facing *= clamp(pow(abs(along), GRADIENT_AMOUNT) * GRADIENT_SHARPNESS, 0.0, 1.0);
    if (abs(along) < 0.000001 || abs(ndv) < 0.000001) {
        facing = 0.0;
    }
    float tail = clamp(1.0 - along, 0.0, 1.0);
    float tail4 = tail * tail * tail * tail;
    sparks *= facing * clamp(ndv, 0.0, 1.0) * min(tail4 * 64.0, 1.0) * 64.0;
    if (tail < 0.000001) {
        sparks = 0.0;
    }

    vec3 c = vec3(COLOR_R, COLOR_G, COLOR_B);
    vec3 base = min(sqrt(c), vec3(1.0));
    base = mix(base, c, clamp(uv.y - 0.5, 0.0, 1.0) * 3.0);
    base = mix(base, clamp(c + 0.1, 0.0, 1.0), inner);
    vec3 rgb = sparks * c + base;
    float alpha = clamp((sparks + facing) * OPACITY, 0.0, 1.0);

    // The game adds alpha * rgb (linear light); Minecraft blends in sRGB space.
    vec3 added = clamp(alpha * rgb, 0.0, 1.0) * (1.0 - total_fog_value(sphericalVertexDistance, cylindricalVertexDistance, FogEnvironmentalStart, FogEnvironmentalEnd, FogRenderDistanceStart, FogRenderDistanceEnd));
    vec4 color = vec4(linearToSrgb(added.r), linearToSrgb(added.g), linearToSrgb(added.b), 1.0) * ColorModulator;

    #ifdef OIT_ALPHA_ONLY
    executeAlphaOnlyPhase(gl_FragCoord.z, 0.0);
    #elif defined(OIT_ACCUMULATE)
    fragColor = sampleColorForAccumulation(color);
    #else
    fragColor = color;
    #endif
}

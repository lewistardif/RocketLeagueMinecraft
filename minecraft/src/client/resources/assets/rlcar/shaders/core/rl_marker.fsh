#version 330
#extension GL_ARB_separate_shader_objects : require

// The ball's markers, translated from the game's compiled pixel shaders (RefShaderCache-PC-D3D-SM5;
// tools/rl_assets/shader_cache.py). The ball's FX actor (FXActors.Ball.Ball_FXActor) attaches them:
//
//   1 GroundDecal: Ball_GroundReticle_DMat (additive), projected straight down onto the ground
//     under the ball. A fixed ring the size of the ball and an inner ring that closes in as the
//     ball climbs (its Altitude parameter, 0 .. 1024 uu), cut by the cross in Reticles_01_Pack (G).
//   2 GroundLinePSC, its reticle: Reticle_Mat (translucent, no depth test), a camera-facing ring
//     the size of the ball, seen through everything once the ball is 2048 .. 4096 uu away.
//   3 GroundLinePSC, its beam: Ball_LocationBeam01_Mat (translucent), a 2 uu wide line from the
//     ball 2500 uu down, dashed 32 times, kept out of the middle of the screen.
//   4 GroundLinePSC, its clarity sphere: BallClaritySphere_Mat (translucent, black) on the inside
//     of a 128 uu sphere around the ball: a dark halo behind the ball from 2048 uu away.
//
// MARKER_KIND picks the material. The game blends linear light; Minecraft blends sRGB values. These
// add or lay small amounts of light over the scene, which in sRGB is closest to adding the linear
// amount as it is, so the colours are not converted (the black halo's opacity is, so that it darkens
// the scene as much as it would in linear light).

#include <minecraft:globals.glsl>
#include <minecraft:fog.glsl>
#include <minecraft:dynamictransforms.glsl>
#include <minecraft:oit.glsl>

uniform sampler2D Sampler0;

layout(location = 0) in float sphericalVertexDistance;
layout(location = 1) in float cylindricalVertexDistance;
layout(location = 2) in vec2 texCoord0;
layout(location = 3) in vec4 vertexColor;
layout(location = 4) in vec3 worldPos;
layout(location = 5) in vec3 worldNormal;
layout(location = 6) in float viewDepth;

#ifndef OIT_ALPHA_ONLY
layout(location = 0) out vec4 fragColor;
#endif

#define UU_PER_BLOCK 100.0

// The marker: colour (linear) and opacity.
vec4 shade(vec2 uv) {
    float depthUu = viewDepth * UU_PER_BLOCK;
#if MARKER_KIND == 1
    // The decal's own box: nothing outside it.
    if (uv.x * (1.0 - uv.x) < 0.0 || uv.y * (1.0 - uv.y) < 0.0) {
        discard;
    }
    vec2 p = uv * 2.0 - 1.0;
    float r2 = dot(p, p);
    // clamp(Altitude / 1024, 0, 1) is in the vertex colour's red.
    float h = vertexColor.r * 0.8375 + 0.125;
    float inner = 1.0 - abs((1.0 - r2 - h - 0.0625) * 16.0);
    float outer = max(1.0 - abs((0.9375 - r2) * 16.0), 0.0);
  #ifdef RETICLE_TEXTURE
    float cut = texture(Sampler0, uv).g;
  #else
    float cut = 0.0;
  #endif
    inner = clamp(inner - cut * 4.0, 0.0, 1.0);
    return vec4(vec3(min(inner + outer, 0.5)), 0.0);
#elif MARKER_KIND == 2
    vec2 p = uv * 2.0 - 1.0;
    float ring = clamp((1.0 - abs((0.875 - dot(p, p)) * 12.0)) * 4.0, 0.0, 1.0);
    float far = max((clamp(depthUu * 0.000244, 0.0, 1.0) - 0.5) * 2.0, 0.0);
    return vec4(vertexColor.rgb, ring * far);
#elif MARKER_KIND == 3
    float dash = clamp((sin(uv.y * 201.061935) - 0.75) * 8.0, 0.0, 0.75) + 0.25;
    vec2 s = gl_FragCoord.xy / ScreenSize - 0.5;
    float aside = clamp((dot(s, s) - 0.0625) * 4.0, 0.0, 1.0);
    return vec4(vertexColor.rgb * 8.0, aside * dash * vertexColor.a);
#elif MARKER_KIND == 4
    float facing = clamp(dot(normalize(worldNormal), normalize(-worldPos)), 0.0, 1.0);
    float a = facing * clamp(depthUu * 0.000488, 0.0, 1.0);
    return vec4(0.0, 0.0, 0.0, a * a);
#else
    return vec4(0.0);
#endif
}

void main() {
    vec4 s = shade(texCoord0);
    float fog = total_fog_value(sphericalVertexDistance, cylindricalVertexDistance, FogEnvironmentalStart, FogEnvironmentalEnd, FogRenderDistanceStart, FogRenderDistanceEnd);
#if MARKER_KIND == 1
    // Additive.
    vec4 color = vec4(clamp(s.rgb, 0.0, 1.0) * (1.0 - fog), 1.0) * ColorModulator;
    #ifdef OIT_ALPHA_ONLY
    executeAlphaOnlyPhase(gl_FragCoord.z, 0.0);
    #elif defined(OIT_ACCUMULATE)
    fragColor = sampleColorForAccumulation(color);
    #else
    fragColor = color;
    #endif
#else
    float alpha = clamp(s.a, 0.0, 1.0) * (1.0 - fog);
    #if MARKER_KIND == 4
    // Black at this opacity in linear light: the sRGB value scales by about (1 - a)^(1/2.2).
    alpha = 1.0 - pow(1.0 - alpha, 1.0 / 2.2);
    #endif
    #ifdef OIT_ALPHA_ONLY
    executeAlphaOnlyPhase(gl_FragCoord.z, alpha);
    #elif defined(OIT_ACCUMULATE)
    // The accumulation weighs the colour by its opacity itself.
    fragColor = sampleColorForAccumulation(vec4(s.rgb, alpha) * ColorModulator);
    #elif MARKER_KIND == 3
    // Premultiplied blending: the line is brighter than white (8) at a low opacity.
    fragColor = vec4(min(s.rgb * alpha, vec3(1.0)), alpha) * ColorModulator;
    #else
    fragColor = vec4(clamp(s.rgb, 0.0, 1.0), alpha) * ColorModulator;
    #endif
#endif
}

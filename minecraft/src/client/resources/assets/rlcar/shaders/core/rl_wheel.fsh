#version 330
#extension GL_ARB_separate_shader_objects : require

// Rocket League's wheels: material Wheel_Master_Mat, translated instruction by instruction from
// the game's compiled pixel shader (same pass as rl_body.fsh). Registers as in the original; the
// lighting comes from rlcar:rl_env.glsl.
//
// The rim is brushed metal (rim normal plus a tiled brushed-metal normal) with a swirling
// reflection and a sharp highlight (SpecPower); the tyre gets a soft rubber sheen. The game tells
// them apart by the mesh's vertex colour (red = tyre), from rl_car.vsh (RL_WHEEL). Tyre and rim
// share the UV space, so no texture can stand in for it; extractions older than the vertex colours
// fall back to the rim's RGB mask, which is wrong on about half of the wheel.
//
// Defines: P58 additional normal scale, P59 RimColor,
// P60 (Rim_AdditionalNormal_Power, ReflectionBrightness, SpecIntensity, SpecPower).

#include <rlcar:rl_env.glsl>

uniform sampler2D RimNormalMap;     // t0 (X in alpha)
uniform sampler2D RimAddNormalMap;  // t1 (X in alpha)
uniform sampler2D TireNormalMap;    // t2
uniform sampler2D SwirlMap;         // t3
uniform sampler2D RimDiffuseMap;    // t4 (sRGB)
uniform sampler2D TireDiffuseMap;   // t5 (sRGB)
uniform sampler2D TireMaskMap;      // the fallback without vertex colours

layout(location = 9) in float vertexTyre;

#define CB58 vec4(P58_X, P58_Y, P58_Z, P58_W)
#define CB59 vec4(P59_X, P59_Y, P59_Z, P59_W)
#define CB60 vec4(P60_X, P60_Y, P60_Z, P60_W)

void main() {
    RlEnv e = rlEnv();
    vec2 v3 = texCoord0;
    vec3 v4 = toTangent(e, e.L);
    vec3 v5 = toTangent(e, e.V);
    vec3 v6 = toTangent(e, vec3(0.0, 1.0, 0.0));
    float v2x = vertexTyre >= 0.0 ? vertexTyre : smoothstep(0.4, 0.6, texture(TireMaskMap, v3).x);
    vec4 r0 = vec4(0.0), r1 = vec4(0.0), r2 = vec4(0.0), r3 = vec4(0.0), r4 = vec4(0.0), r5 = vec4(0.0);

    r0.x = inversesqrt(dot(v4, v4));
    r0.yzw = r0.x * v4;
    r1.xyz = texture(TireNormalMap, v3).xyz * 2.0 - 1.0;
    r2.xyz = r1.xyz * vec3(0.625, 0.625, 1.0);
    r0.y = clamp(dot(r2.xyz, r0.yzw), 0.0, 1.0);
    r0.z = inversesqrt(dot(v5, v5));
    r3.xyz = r0.z * v5;
    r0.z = dot(r2.xyz, r3.xyz);
    r0.w = abs(r0.z) * abs(r0.z);
    r1.w = r0.w * r0.w;
    r0.w = r0.w * r1.w + 0.015625;
    r0.w *= 0.5;
    r0.w = abs(r0.z) < 0.000001 ? 0.007812 : r0.w;
    r2.xyz = srgbToLinear(texture(TireDiffuseMap, v3).xyz);
    r4.xyz = r2.xyz * 16.0;
    r2.xyz = r0.y * r2.xyz;
    r2.xyz = r0.z * r2.xyz;
    r4.xyz = r0.w * r4.xyz;
    r5.xyz = r0.y * CB59.xyz - r4.xyz;
    r0.y = r0.z + r0.z;
    r0.z = clamp((0.625 - r0.z) * 1.666, 0.0, 1.0);
    r0.y = min(max(r0.y, 0.35), 1.0);
    r0.y = r0.z * r0.y;
    r0.y = r0.y * r0.y;
    r0.yzw = clamp(r0.y * r5.xyz + r4.xyz, 0.0, 1.0);

    // Rim normal: the rim's own plus the brushed metal, tiled along the rim.
    vec4 add = texture(RimAddNormalMap, vec2(v3.y * 10.0, v3.x));
    r4.xyz = vec3(add.w, add.y, add.z) * 2.0 - 1.0;
    vec4 rim = texture(RimNormalMap, v3);
    r5.xyz = vec3(rim.w, rim.y, rim.z) * 2.0 - 1.0;
    r4.xyz = r4.xyz * CB58.xyz + r5.xyz;
    r1.xyz = r1.xyz * vec3(0.625, 0.625, 1.0) - r4.xyz;
    r1.xyz = v2x * r1.xyz + r4.xyz;
    r1.xyz = normalize(r1.xyz);
    r1.w = dot(r1.xyz, r3.xyz);
    r4.xy = r1.w * r1.xy * 2.0 - r3.xy;
    r3.xyz = v4 * r0.x + r3.xyz;
    r4.xy *= 0.05;

    // Reflection swirl over the rim colour.
    r0.x = texture(SwirlMap, r4.xy).x + CB60.y;
    r4.xyz = srgbToLinear(texture(RimDiffuseMap, v3).xyz);
    r0.yzw = -r0.x * r4.xyz + r0.yzw;
    r5.xyz = r0.x * r4.xyz;
    r4.xyz = clamp(r4.xyz * CB60.z, 0.0, 1.0);
    r0.xyz = v2x * r0.yzw + r5.xyz;
    r2.xyz = r2.xyz * 4.0 - r4.xyz;
    r2.xyz = v2x * r2.xyz + r4.xyz;

    // Highlight.
    r3.xyz = normalize(r3.xyz);
    r0.w = clamp(dot(r1.xyz, r3.xyz), 0.0, 1.0);
    r2.w = 6.0 - CB60.w;
    r2.w = v2x * r2.w + CB60.w;
    r1.w = exp2(log2(r0.w) * r2.w);
    bool zero = r0.w < 0.000001;
    r0.w = zero ? 0.0 : r0.w;
    r1.w = zero ? 0.0 : r1.w;
    r3.xyz = r2.xyz * r1.w;
    r2.xyz = r0.xyz * r0.w + r3.xyz;

    vec3 normalWorld = toWorld(e, r1.xyz);
    vec3 color = skyLight(e, r0.xyz, dot(normalize(v6), r1.xyz));
    color = r0.xyz * max(shIrradiance(e, normalWorld), 0.0) + color;
    vec3 dbgDirect = r2.xyz * e.lightColor;
    color = dbgDirect + color;
    color = r0.xyz * ambientColor(e) + color;
#if defined(RL_DEBUG_1)
    fragColor = rlFinish(r0.xyz);
#elif defined(RL_DEBUG_2)
    fragColor = rlFinish(dbgDirect);
#elif defined(RL_DEBUG_3)
    fragColor = rlFinish(color - dbgDirect);
#elif defined(RL_DEBUG_4)
    fragColor = rlFinish(vec3(0.0));
#else
    fragColor = rlFinish(color);
#endif
}

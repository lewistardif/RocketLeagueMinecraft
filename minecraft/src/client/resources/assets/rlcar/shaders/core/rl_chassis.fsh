#version 330
#extension GL_ARB_separate_shader_objects : require

// Rocket League's car chassis (lights, underbody, exhausts, grilles): material MasterChassis_MAT,
// translated instruction by instruction from the game's compiled pixel shader (same pass as
// rl_body.fsh). Registers as in the original; the lighting comes from rlcar:rl_env.glsl.
//
// What it does: the diffuse is lit through the LightFalloffArray ramp (row 8 and below, picked by
// Masks.B) with a reflection term from the cube map; Masks.R marks the lights, which glow in the
// head or tail light colour (Masks.A > 0.5 is a headlight) and get brighter with distance so they
// read from across the arena; Masks.G glows when braking.
//
// Defines: P59 TailLightColor, P60 HeadlightColor, P61 BoostGlowColor, BRAKE.

#include <rlcar:rl_env.glsl>

uniform sampler2D LightRampMap; // t0 LightFalloffArray
uniform sampler2D DiffuseMap;   // t1 (sRGB)
uniform sampler2D MasksMap;     // t2

#define CB59 vec4(P59_X, P59_Y, P59_Z, P59_W)
#define CB60 vec4(P60_X, P60_Y, P60_Z, P60_W)
#define CB61 vec4(P61_X, P61_Y, P61_Z, P61_W)

void main() {
    RlEnv e = rlEnv();
    vec2 v3 = texCoord0;
    vec3 v4 = toTangent(e, e.L);
    vec3 v5 = toTangent(e, e.V);
    vec3 v6 = toTangent(e, vec3(0.0, 1.0, 0.0));
    float v7w = e.depthUu;
    vec4 r0 = vec4(0.0), r1 = vec4(0.0), r2 = vec4(0.0), r3 = vec4(0.0), r4 = vec4(0.0), r5 = vec4(0.0), r6 = vec4(0.0), r7 = vec4(0.0), r8 = vec4(0.0);

    r0.x = inversesqrt(dot(v4, v4));
    r0.y = inversesqrt(dot(v5, v5));
    r1.xyz = v5 * r0.y;
    r0.yz = vec2(-v5.z * r0.y + 1.0, -v5.z * r0.y + 1.125);
    r2.xyz = v4 * r0.x + r1.xyz;
    r0.x = max(r0.x * v4.z, 0.0);
    r2.xyz *= 0.5;
    r0.w = sqrt(dot(r2.xyz, r2.xyz));
    r2.z = r2.z / r0.w;
    r2.yw = vec2(0.25, 0.25);
    r3 = texture(MasksMap, v3);
    r2.w = r3.z * 0.5 + r2.w;
    r0.w = texture(LightRampMap, r2.zw).x;
    r2.zw = vec2(r3.w - 0.5, r3.z + 4.0);
    r0.z = r0.z * r2.w;
    r1.w = clamp(r2.z + r2.z, 0.0, 1.0);
    r0.z = r0.z * r0.w;
    // The game samples its reflection cube with this tangent-space vector.
    r4.xyz = vec3(-r1.x, -r1.y, r1.z);
    r0.w = dot(envCube(e, vec3(r4.x, r4.z, r4.y)), vec3(0.2126, 0.7152, 0.0722));
    r0.y = r0.w * r0.y - 0.0625;
    r0.w = r3.z * 0.5 + 2.0;
    r0.y = r0.w * r0.y;
    r0.y = clamp(r0.y * 8.0, 0.0, 1.0);
    r0.w = r0.y * 0.125;
    r0.y = r0.y * 0.5 + r0.z;
    r0.z = r0.z * 0.5 + r0.w;
    r0.y *= 0.5;

    // Lights: head or tail colour where Masks.R is set, the diffuse elsewhere.
    r1.x = clamp(r3.x * 512.0, 0.0, 1.0);
    vec3 lc = r1.w * (CB60.xyz - CB59.xyz) + CB59.xyz;
    r0.w = -r3.y * 0.5 + 1.0;
    r4 = texture(DiffuseMap, v3);
    r4.xyz = srgbToLinear(r4.xyz);
    r4.xyz *= r4.w;
    r1.y = 1.0 - r4.w;
    lc = -r0.w * r4.xyz + lc;
    r5.xyz = r0.w * r4.xyz;
    lc = r1.x * lc + r5.xyz;
    r0.w = clamp(v7w * 0.000244, 0.0, 1.0) + 1.0;
    lc *= r0.w;
    r6.xyz = r0.y * lc + r0.z;
    vec3 r0yzw = r0.y * lc;
    r7.xyz = r1.w * (CB60.xyz - CB61.xyz) + CB61.xyz;
    r8.xyz = r1.z * lc;
    r5.xyz = r5.xyz * r8.xyz;
    vec4 lt = texture(LightRampMap, vec2(r1.z, r2.y));
    r2.xyz = lt.yzw;
    r7.xyz = -r2.z * r5.xyz + r7.xyz;
    r5.xyz *= r2.z;
    vec3 r2xzw = r2.x * r7.xyz + r5.xyz;
    lc *= r2.y;
    vec3 r1xyw = r1.y * r6.xyz + r2xzw;
    r2.y = min(r0.x, 1.0);
    r1xyw *= r2.y;
    r1xyw *= 1.5;
    r1xyw = clamp(r1xyw, 0.0, 4.0);
    r0.x = r1.z * r1.z;
    r0.xyz = r0.x * r5.xyz + r0yzw;
    r0.xyz += r2xzw;
    r0.xyz = clamp(r0.xyz, 0.0, 4.0);

    // Emissive: the lights, and the brake glow (Masks.G).
    r4.xyz = r4.xyz * BRAKE * r3.y * 16.0;
    vec3 emissive = r1.z * lc + r4.xyz;

    vec3 normalWorld = e.N;
    vec3 color = skyLight(e, r0.xyz, normalize(v6).z) + emissive;
    color = r0.xyz * max(shIrradiance(e, normalWorld), 0.0) + color;
    color = r1xyw * e.lightColor + color;
    color = r0.xyz * ambientColor(e) + color;
    fragColor = rlFinish(color);
}

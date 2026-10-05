#version 330
#extension GL_ARB_separate_shader_objects : require

// Rocket League's car chassis (underbody, engine, exhausts, grilles, lights), translated
// instruction by instruction from the game's compiled pixel shaders (RefShaderCache-PC-D3D-SM5,
// base pass with the SH + directional light policy and sky light; disassembled with
// tools/rl_assets/shader_cache.py). Three base materials, one program:
//
//   MasterChassis_MAT (Octane, Hybrid, Merc, Plank; also     default
//     the Psyclops' GoodChassis_Painted_Mat, whose shader
//     map the tool cannot read, with no swirl texture)
//   MAT_Chassis_Paintable (Breakout)                          PAINTABLE
//   MAT_BANDAID_Chassis_Paintable (Dominus)                   PAINTABLE + BANDAID
//
// What they do: the diffuse with a normal map plus a tiled brushed-metal detail normal (weighted
// by the diffuse alpha); a reflection (MasterChassis: the Swirls texture looked up by the world
// reflection's horizontal direction; the paintable ones: the reflection cube, with a paint / trim
// mix driven by the normal map's red channel); a sharp highlight; and the lights, which are
// emissive where Masks.A is set: tail lights (Masks.R, brighter when braking), headlights
// (Masks.G) and the boost glow (Masks.B, scaled by BoostGlowIntensity, which the game raises from
// native code while boosting and which stays at the material's value here).
//
// Defines: P58 TailLightColor, P59 HeadlightColor, P60 BoostGlowColor, P61 TrimColor, BRAKE,
// TAIL_BRIGHTNESS, HEAD_BRIGHTNESS, GLOW_BRIGHTNESS, GLOW_INTENSITY, TRIM_EMISSIVE.

#include <rlcar:rl_env.glsl>

uniform sampler2D NormalMap;   // t0 (X in alpha; the paintable ones use red as the trim mask)
uniform sampler2D DetailMap;   // t1 BrushedMetal_Normal (X in alpha)
uniform sampler2D DiffuseMap;  // t2 (sRGB)
uniform sampler2D SwirlMap;    // t3 Swirls_D (MasterChassis only)
uniform sampler2D MasksMap;    // t4 (t3 in the paintable ones)

#define TAIL_COLOR vec3(P58_X, P58_Y, P58_Z)
#define HEAD_COLOR vec3(P59_X, P59_Y, P59_Z)
#define GLOW_COLOR vec3(P60_X, P60_Y, P60_Z)
#define TRIM_COLOR vec4(P61_X, P61_Y, P61_Z, P61_W)

void main() {
    RlEnv e = rlEnv();
    vec2 uv = texCoord0;
    vec3 V = normalize(toTangent(e, e.V));
    vec3 L = normalize(toTangent(e, e.L));
    vec3 sky = normalize(toTangent(e, vec3(0.0, 1.0, 0.0)));

    vec4 nt = texture(NormalMap, uv);
    vec3 n0 = vec3(nt.w, nt.y, nt.z) * 2.0 - 1.0;
    vec4 dt = texture(DetailMap, vec2(uv.y * 10.0, uv.x));
    vec3 detail = (vec3(dt.w, dt.y, dt.z) * 2.0 - 1.0) * vec3(0.025, 0.025, 0.0);
    vec4 tex = texture(DiffuseMap, uv);
    vec3 diffuse = srgbToLinear(tex.rgb);
    vec3 n = normalize(tex.a * detail + n0);
    vec3 R = n * dot(n, V) * 2.0 - V;
    vec4 masks = texture(MasksMap, uv);

#ifndef PAINTABLE
    // Rim light: (1 - N.V)^7 of the undetailed normal, in the diffuse's red.
    float a = 1.0 - max(dot(n0, V), 0.0);
    float rim = abs(a) < 0.000001 ? 0.0 : exp2(log2(abs(a)) * 7.0);
    // Reflection: Swirls_D at the world reflection's horizontal direction (Unreal axes).
    float refl = texture(SwirlMap, ueAxes(toWorld(e, R)).xy * 0.1).x;
    vec3 D = diffuse * refl + diffuse;
    D = rim * diffuse.r + D;
    vec3 specColor = clamp(diffuse * 16.0 - refl, 0.0, 1.0);
    float specPower = 96.0;
    vec3 emissiveBase = diffuse * refl;
#else
    float mk = nt.x;
    float ndv = dot(n0, V);
    vec3 cube = envCube(e, toWorld(e, R)) * 2.0;
    vec3 base = mix(diffuse, vec3(0.5), mk) * (1.0 - mk);
  #ifdef BANDAID
    // The trims' Fresnel (as on the body's windows and trims), blended by TrimColor.a.
    float b = 1.0 - max(V.z, 0.0);
    float f5 = abs(b) < 0.000001 ? 0.0 : min(abs(b) * abs(b) * abs(b) * abs(b) * abs(b), 1.0);
    float edge15 = abs(V.z) < 0.000001 ? 0.0 : min(min(exp2(log2(abs(V.z)) * 15.0), 1.0) * 2.0, 1.5);
    float face4 = abs(V.z) < 0.000001 ? 0.0 : min(V.z * V.z * V.z * V.z, 1.0) * 0.25;
    float fa = min(f5 + 0.1, 1.0) + edge15;
    float fb = face4 + f5 + 0.5;
    float trim = TRIM_COLOR.w * (fb - fa) + fa;
    vec3 trimmed = mix(base, trim * TRIM_COLOR.rgb, mk);
  #else
    vec3 trimmed = mix(base, masks.w * TRIM_COLOR.rgb, mk);
  #endif
    vec3 D = mix(trimmed, cube * base + diffuse, 1.0 - mk);
    float s = clamp(1.15 - mk, 0.0, 1.0);
    float edge = 1.0 - clamp(ndv + ndv, 0.0, 1.0);
    D = edge * edge * diffuse.r + D;
    D *= ndv * ndv * ndv * ndv + 0.5;
    vec3 specColor = mix(clamp(diffuse * 16.0, 0.0, 1.0), vec3(0.5), masks.y) * s;
    float specPower = 96.0 * s;
    vec3 emissiveBase = vec3(0.0);
#endif

    // The lights: tail lights brighter when braking, headlights, the boost glow; where Masks.A is set.
    float tail = clamp(BRAKE, 0.0, 1.0) * (20.0 - TAIL_BRIGHTNESS) + TAIL_BRIGHTNESS;
    vec3 lights = diffuse * masks.x * TAIL_COLOR * tail + diffuse * masks.y * HEAD_BRIGHTNESS * HEAD_COLOR;
    lights += clamp(GLOW_INTENSITY, 0.0, 2.0) * 5.0 * masks.z * GLOW_COLOR * GLOW_BRIGHTNESS;
    vec3 emissive = masks.w * lights + emissiveBase;
#ifdef BANDAID
    emissive = mix(emissive, TRIM_EMISSIVE * TRIM_COLOR.rgb, clamp(nt.x, 0.0, 1.0));
#endif

    vec3 H = normalize(L + V);
    float ndh = clamp(dot(n, H), 0.0, 1.0);
    float spec = ndh < 0.000001 ? 0.0 : exp2(log2(ndh) * specPower);
    ndh = ndh < 0.000001 ? 0.0 : ndh;
    vec3 direct = D * ndh + specColor * spec;

    vec3 color = skyLight(e, D, dot(sky, n)) + emissive;
    color = D * max(shIrradiance(e, toWorld(e, n)), 0.0) + color;
    color = direct * e.lightColor + color;
    color = D * ambientColor(e) + color;
#if defined(RL_DEBUG_1)
    fragColor = rlFinish(D);
#elif defined(RL_DEBUG_2)
    fragColor = rlFinish(direct * e.lightColor);
#elif defined(RL_DEBUG_3)
    fragColor = rlFinish(color - direct * e.lightColor);
#elif defined(RL_DEBUG_4)
    fragColor = rlFinish(emissive);
#else
    fragColor = rlFinish(color);
#endif
}

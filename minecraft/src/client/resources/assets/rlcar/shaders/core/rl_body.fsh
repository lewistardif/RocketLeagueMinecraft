#version 330
#extension GL_ARB_separate_shader_objects : require

// Rocket League's car body paint: material Body_Paintable_Mat, translated instruction by
// instruction from the game's compiled pixel shader (RefShaderCache-PC-D3D-SM5, base pass with
// the SH + directional light policy and sky light; static switches CarParts, CurvaturePack,
// Glass, PaintMaskInRGB, Rubber, Sparkle, TrimPaintable, UseDetailNormals, UseDiffuseTexture,
// UsePaintedSkin). Register names follow the original (r0..r20, v3 = UV, v4/v5/v6 = light,
// camera and sky vectors in tangent space). The lighting comes from rlcar:rl_env.glsl.
//
// What it does: the paint, team, accent and trim colours are mixed by the Skin and CurvaturePack
// masks; every lighting term goes through the LightFalloffArray ramp atlas (32 rows of 4 texels,
// one row per finish, F1Type / F2Type), which gives Rocket League's soft wrap-around diffuse and
// its tight clear-coat highlights; reflections come from the reflection cube and the ENVPack
// texture; CurvaturePack.G marks the windows and trims, drawn in TrimColor with a Fresnel rim;
// BodyMasks.a blends in a second ("tertiary") material with its own normal map and ramp row.
//
// Material parameters come in as defines P<register>_<component> (the game's cb0 registers):
// 58 TertiaryMaterial_ControlA, 59 TeamColor, 60 CustomColor, 61 F1ControlA, 62 F2ControlA,
// 63 F1ControlB, 64 F2ControlB, 65 PaintColor, 66 TrimColor, 67 TertiaryMaterial_Color,
// 68 TertiaryMaterial_ControlB, 69 (TertiaryNormalTiling, -, -, F1Type / 32),
// 70 (-, -, F2Type / 32, TertiaryMaterial_Type).

#include <rlcar:rl_env.glsl>

uniform sampler2D TertiaryNormalMap; // t0
uniform sampler2D BodyMaskMap;       // t1
uniform sampler2D DiffuseMap;        // t2 (sRGB)
uniform sampler2D SkinMap;           // t3
uniform sampler2D PartsNormalMap;    // t4 (X in alpha)
uniform sampler2D Detail1Map;        // t5
uniform sampler2D Detail2Map;        // t6
uniform sampler2D LightRampMap;      // t7 LightFalloffArray
uniform sampler2D CurvatureMap;      // t8
uniform sampler2D EnvPackMap;        // t9 ENVPack

#define CB58 vec4(P58_X, P58_Y, P58_Z, P58_W)
#define CB59 vec4(P59_X, P59_Y, P59_Z, P59_W)
#define CB60 vec4(P60_X, P60_Y, P60_Z, P60_W)
#define CB61 vec4(P61_X, P61_Y, P61_Z, P61_W)
#define CB62 vec4(P62_X, P62_Y, P62_Z, P62_W)
#define CB63 vec4(P63_X, P63_Y, P63_Z, P63_W)
#define CB64 vec4(P64_X, P64_Y, P64_Z, P64_W)
#define CB65 vec4(P65_X, P65_Y, P65_Z, P65_W)
#define CB66 vec4(P66_X, P66_Y, P66_Z, P66_W)
#define CB67 vec4(P67_X, P67_Y, P67_Z, P67_W)
#define CB68 vec4(P68_X, P68_Y, P68_Z, P68_W)
#define CB69 vec4(P69_X, P69_Y, P69_Z, P69_W)
#define CB70 vec4(P70_X, P70_Y, P70_Z, P70_W)

vec4 ramp(vec2 uv) {
    return texture(LightRampMap, uv);
}

void main() {
    RlEnv e = rlEnv();
    vec2 v3 = texCoord0;
    vec3 v4 = toTangent(e, e.L);
    vec3 v5 = toTangent(e, e.V);
    vec3 v6 = toTangent(e, vec3(0.0, 1.0, 0.0));
    float v7w = e.depthUu;
    vec4 r0 = vec4(0.0), r1 = vec4(0.0), r2 = vec4(0.0), r3 = vec4(0.0), r4 = vec4(0.0), r5 = vec4(0.0), r6 = vec4(0.0);
    vec4 r7 = vec4(0.0), r8 = vec4(0.0), r9 = vec4(0.0), r10 = vec4(0.0), r11 = vec4(0.0), r12 = vec4(0.0), r13 = vec4(0.0);
    vec4 r14 = vec4(0.0), r15 = vec4(0.0), r16 = vec4(0.0), r17 = vec4(0.0), r18 = vec4(0.0), r19 = vec4(0.0), r20 = vec4(0.0);

    // Tertiary material normal, and the normal blended by its mask.
    r3.x = CB58.w;
    r3.yw = v3 * CB69.x * 0.25;
    r4 = texture(TertiaryNormalMap, r3.yw);
    r5.xyz = r4.xyz * 2.0 - 1.0;
    r3.z = 1.0;
    r6.xyz = vec3(r3.x * r5.x, r3.x * r5.y, r3.z * r5.z - 1.0);
    r3.xyw = vec3(r3.x * r5.x, r3.x * r5.y, r3.z * r5.z);
    r0.w = clamp(-r3.z * r5.z + 1.0, 0.0, 1.0);
    r1.w = texture(BodyMaskMap, v3).w;
    r6.xyz = r1.w * r6.xyz + vec3(0.0, 0.0, 1.0);
    r7.xyz = normalize(r6.xyz);
    r8.xyz = normalize(v5);
    r2.w = dot(r7.xyz, r8.xyz);
    r9.xyz = r2.w * r7.xyz * 2.0 - r8.xyz;

    // Reflection vector and normal in world space; the reflection cube.
    vec3 reflWorld = toWorld(e, r9.xyz);
    vec3 normalWorld = toWorld(e, r7.xyz);
    r10.xyz = ueAxes(reflWorld);
    r0.xyz = envCube(e, reflWorld);
    r1.xy = r10.xy + 1.0;

    // Car parts normal (X in alpha) and the two finishes' detail normals.
    vec4 t4 = texture(PartsNormalMap, v3);
    r9.xyz = vec3(t4.w, t4.y, t4.z) * 2.0 - 1.0;
    r11 = texture(Detail2Map, v3 * 4.0);
    r10 = texture(Detail1Map, v3 * 4.0);
    r11.xyz = r11.xyz * 2.0 - 1.0;
    r1.z = r11.w - r10.w;
    r10.xyz = r10.xyz * 2.0 - 1.0;
    r11.xyz = r11.xyz - r10.xyz;
    r12 = texture(SkinMap, v3);
    r10.xyz = r12.w * r11.xyz + r10.xyz;
    r10.xyz += vec3(0.0, 0.0, -1.0);
    r11 = CB62 - CB61;
    r11 = r12.w * r11 + CB61;
    r11 *= r12.x;
    r13.xyz = r11.w * r10.xyz + vec3(0.0, 0.0, 1.0);
    r10.xyz = r11.z * r10.xyz + vec3(0.0, 0.0, 1.0);
    r10.xyz = r10.xyz * vec3(1.0, 1.0, 0.0) + r9.xyz;
    r9.xyz = r13.xyz * vec3(1.0, 1.0, 0.0) + r9.xyz;

    // ENVPack, looked up by the world reflection's horizontal direction.
    r13.xy = r1.xy * 0.5 + r9.xy;
    r1.xy = r1.xy * 0.5 + r3.xy;
    r14.xyz = texture(EnvPackMap, r1.xy).xyw;
    r13.xyz = texture(EnvPackMap, r13.xy).xyw;
    r1.x = r13.y - r13.x;

    // Facing ratio through the finish's ramp rows.
    r15.xyz = r10.xyz + vec3(0.0, 0.0, -1.0);
    r1.y = r11.y * 0.5;
    r15.xyz = r1.y * r15.xyz + vec3(0.0, 0.0, 1.0);
    r15.x = dot(r15.xyz, r8.xyz);
    r16.w = r12.z * 0.5 + CB69.w;
    r15.y = r16.w + 0.015626;
    r17 = ramp(r15.xy);
    r18.z = r12.z * 0.5 + CB70.z;
    r15.z = r18.z + 0.015626;
    r19 = ramp(r15.xz);
    r19 -= r17;
    r17 = r12.w * r19 + r17;
    r16.x = r17.w * r1.x + r13.x;
    r18.y = r16.w;
    r18.w = r16.x;
    r1.x = ramp(vec2(r16.x, r16.w)).z;
    r1.y = ramp(vec2(r18.w, r18.z)).z;
    r1.y = r1.y - r1.x;
    r1.x = r12.w * r1.y + r1.x;
    r16.y = r12.w * -2.0 + 1.0;
    r16.z = r12.z * 0.5 + 0.253908;
    r1.y = ramp(vec2(r16.x, r16.z)).z;
    r1.x = r1.x - r1.y;
    r1.x = r12.x * r1.x + r1.y;

    // Finish controls B (lerped by the finish mask), chrome overrides.
    vec3 fb = r12.w * (CB64.xyz - CB63.xyz) + CB63.xyz;
    r13.x = r12.x * (fb.x - 0.125) + 0.125;
    r13.y = r12.x * (fb.y - 0.015625) + 0.015625;
    r13.w = r12.x * fb.z;
    r15.y = 8.0 - r13.x;
    r15.z = 0.125 - r13.y;
    r13.xy = r12.z * r15.yz + r13.xy;
    r1.y = r1.x * r13.y;
    r2.w = r15.x - 0.09375;
    r3.z = -r15.x * r15.x + 1.25;
    r2.w = clamp(r2.w * 12.0, 0.0, 1.0);
    r2.w = r2.w * r3.z;
    r3.z = r1.y * 3.0 + r2.w;
    r5.w = r17.x - 0.5;
    r6.w = clamp(r5.w + r5.w, 0.0, 1.0);
    r5.w = clamp(r5.w * 8.0 + 0.75, 0.0, 1.0);
    r6.w = r6.w + r17.z;
    r6.w = r6.w * 6.0 + 0.3;
    r3.z = r3.z * r6.w;
    r15.xyz = r0.xyz * r3.z;
    r15.xyz = r12.z * r15.xyz + r1.y;
    r1.y = clamp(1.0 - r9.z, 0.0, 1.0);
    r1.y = r13.z * r1.y;
    r13.yzw = r1.y * r13.w + r15.xyz;
    r15.z = r18.y;
    r15.w = r18.z;

    // Specular: half vector through the ramp rows.
    float il = inversesqrt(dot(v4, v4));
    r19.xyz = v4 * il + r8.xyz;
    r20.xyz = v4 * il;
    r19.xyz *= 0.5;
    r19.xyz = r19.xyz / sqrt(dot(r19.xyz, r19.xyz));
    r3.z = dot(r9.xyz, r19.xyz);
    r3.x = dot(r3.xyw, r19.xyz);
    r9.x = r3.x * r3.x;
    r15.x = r3.z * r3.z;
    r3.x = ramp(r15.xw).y;
    r3.y = ramp(r15.xz).y;
    r3.x = r3.x - r3.y;
    r3.x = r12.w * r3.x + r3.y;
    r15.y = r16.z;
    r3.y = r16.y * r17.z;
    r3.z = ramp(r15.xy).y;
    r3.x = r3.x - r3.z;
    r3.x = r12.x * r3.x + r3.z;
    float spec = r3.x * r13.x;
    r3.xzw = vec3(spec + r13.y, spec + r13.z, spec + r13.w);

    // Fresnel terms of the windows and trims.
    r7.w = abs(r8.z) * abs(r8.z);
    r7.w = r7.w * r7.w;
    r7.w = min(r7.w, 1.0);
    r7.w *= 0.25;
    bool facingZero = abs(r8.z) < 0.000001;
    r7.w = facingZero ? 0.0 : r7.w;
    r11.w = 1.0 - max(r8.z, 0.0);
    r13.x = abs(r11.w) * abs(r11.w);
    r13.x = r13.x * r13.x;
    r13.x = abs(r11.w) * r13.x;
    bool edgeZero = abs(r11.w) < 0.000001;
    r13.x = min(r13.x, 1.0);
    r11.w = edgeZero ? 0.0 : r13.x;
    r13.xy = r11.w + vec2(0.1, 0.5);
    r7.w = r7.w + r13.y;
    r11.w = min(r13.x, 1.0);
    r13.x = exp2(log2(abs(r8.z)) * 15.0);
    r13.x = min(r13.x, 1.0);
    r13.x = r13.x + r13.x;
    r13.x = min(r13.x, 1.5);
    r8.w = facingZero ? 0.0 : r13.x;
    r8.w = r8.w + r11.w;
    r7.w = r7.w - r8.w;
    r7.w = CB66.w * r7.w + r8.w;

    // Base colour: team / custom / paint colours over the diffuse, windows and trims on top.
    vec3 fbw = r12.w * (CB64.wyz - CB63.wyz) + CB63.wyz;
    r8.w = r12.x * fbw.x;
    r13.x = r12.x * (fbw.y - 0.015625) + 0.015625;
    r13.y = r12.x * fbw.z;
    r3.y = r8.w * r3.y + r12.w;
    r16.xyz = r3.y * (CB60.xyz - CB59.xyz) + CB59.xyz;
    r16.xyz = r12.y * (CB65.xyz - r16.xyz) + r16.xyz;
    r19 = texture(DiffuseMap, v3);
    r19.xyz = srgbToLinear(r19.xyz);
    r3.y = 1.0 - r12.z;
    r16.xyz = r3.y * r16.xyz - r19.xyz;
    r3.y = r3.y * r11.x;
    r16.xyz = r12.x * r16.xyz + r19.xyz;
    vec3 r17xzw = r7.w * CB66.xyz - r16.xyz;
    vec4 t8 = texture(CurvatureMap, v3);
    r11.x = t8.x;
    r11.w = t8.y;
    r16.xyz = r11.w * r17xzw + r16.xyz;
    r7.w = 1.0 - r5.w;
    r5.w = r11.w * r7.w + r5.w;
    r7.w = 1.0 - r11.x;
    r17xzw = r5.w * r16.xyz - 0.5;
    r16.xyz = r16.xyz * r5.w;
    r19.xyz = r3.y * r17xzw + 0.5;
    vec3 r11xyw = r11.y * r17xzw + 0.5;
    r11xyw = r11xyw * r17.y;
    r11xyw = r7.w * r11xyw + r16.xyz;
    r3.xyz = r3.xzw * r19.xyz;

    // Diffuse: half-Lambert through the ramp rows.
    r3.w = dot(r10.xyz, r20.xyz) + 1.0;
    r15.x = r3.w * 0.5;
    r3.w = ramp(r15.xw).x;
    r5.w = ramp(r15.xz).x;
    r3.w = r3.w - r5.w;
    r3.w = r12.w * r3.w + r5.w;
    r3.xyz = r3.w * r11xyw + r3.xyz;

    // Occlusion (diffuse alpha) through the ramp, faded out with distance.
    r1.z = r12.w * r1.z + r10.w;
    r1.z = 1.0 - r1.z;
    r1.z = -r1.z * r11.z + 1.0;
    r18.x = r1.z * r19.w;
    r1.z = ramp(r18.xz).w;
    r3.w = ramp(r18.xy).w;
    r1.z = r1.z - r3.w;
    r1.z = r12.w * r1.z + r3.w;
    r1.z = clamp(v7w * 0.000244 + r1.z, 0.0, 1.0);
    r3.xyz *= r1.z;
    r3.w = abs(r12.w - 0.5) * 2.0;
    r3.xyz = clamp(r3.xyz * r3.w, 0.0, 1.0);

    // Tertiary material, lit through its own ramp row.
    r4.xyz = r4.xyz * 2.0 + vec3(-1.0, -1.0, -2.0);
    r3.w = 1.0 - r4.w;
    r3.w = -r3.w * CB58.z + 1.0;
    r10.x = r3.w * r19.w;
    r3.w = CB58.y * 0.5;
    r4.xyz = r3.w * r4.xyz + vec3(0.0, 0.0, 1.0);
    r4.x = dot(r4.xyz, r8.xyz);
    r4.y = CB70.w * 0.03125 + 0.019534;
    r4.w = CB70.w * 0.03125 + 0.003908;
    vec4 lt = ramp(r4.xy);
    r8.xyz = vec3(lt.x, lt.y, lt.w);
    r3.w = r14.y - r14.x;
    r9.z = r8.z * r3.w + r14.x;
    r0.w = r0.w * r14.z;
    r0.w = r0.w * CB68.z;
    r9.y = CB70.w * 0.03125 + 0.003908;
    r9.w = r9.y;
    r3.w = ramp(r9.zw).z;
    r4.x = ramp(r9.xy).y;
    r0.w = CB68.y * r3.w + r0.w;
    r0.w = r4.x * CB68.x + r0.w;
    r3.w = clamp((r8.x - 0.5) * 8.0 + 0.75, 0.0, 1.0);
    vec3 r8xzw = r3.w * CB67.xyz - 0.5;
    r9.xyz = r3.w * CB67.xyz;
    vec3 r12xyw = CB58.x * r8xzw + 0.5;
    r8xzw = CB58.y * r8xzw + 0.5;
    r8.xyz = r8xzw * r8.y;
    r8.xyz = r19.w * r8.xyz + r9.xyz;
    r9.xyz = r0.w * r12xyw;
    r0.w = dot(r5.xyz, r20.xyz);
    r0.w += 1.0;
    r4.z = r0.w * 0.5;
    r0.w = ramp(r4.zw).x;
    r4.xyz = r0.w * r8.xyz + r9.xyz;
    r10.y = CB70.w * 0.03125 + 0.003908;
    r0.w = clamp(ramp(r10.xy).w, 0.0, 1.0);
    r4.xyz = clamp(r4.xyz * r0.w, 0.0, 1.0);
    r6.xyz = r4.xyz - r3.xyz;
    r3.xyz = r1.w * r6.xyz + r3.xyz;

    // Indirect: the colour the ambient, sky light and SH light.
    r0.w = 0.125 - r13.x;
    r0.w = r12.z * r0.w + r13.x;
    r0.w = r0.w * r1.x;
    r1.x = r0.w * 3.0 + r2.w;
    r1.x = r6.w * r1.x;
    r0.xyz = r0.xyz * r1.x;
    r0.xyz = r12.z * r0.xyz + r0.w;
    r0.xyz = r1.y * r13.y + r0.xyz;
    r0.xyz = r0.xyz * r19.xyz + r11xyw;
    r5.xyz = r0.xyz * r1.z;
    r0.xyz = -r1.z * r0.xyz + r4.xyz;
    r0.xyz = r1.w * r0.xyz + r5.xyz;

    vec3 color = skyLight(e, r0.xyz, dot(normalize(v6), r7.xyz));
    color = r0.xyz * max(shIrradiance(e, normalWorld), 0.0) + color;
    color = r3.xyz * e.lightColor + color;
    color = r0.xyz * ambientColor(e) + color;

    fragColor = rlFinish(color);
}

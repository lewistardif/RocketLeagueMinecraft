#ifndef RLCAR_ENV_GLSL
#define RLCAR_ENV_GLSL

// The lighting environment of the ports of Rocket League's car materials (rl_body, rl_chassis,
// rl_wheel, rl_glass, rl_basic). The game's compiled pixel shaders read their light from engine
// constants: one directional light, a spherical-harmonic ambient, an upper/lower sky light, an
// ambient colour and a reflection cube map. Here they come from Minecraft instead:
//   directional light  the sun (or the moon), dimmed by night, rain and the block's sky light
//   ambient + SH       the lightmap colour at the car (sky and block light, night, the Nether...)
//   sky light          the sky colour above, a ground bounce below
//   reflection cube    a sky dome built from the sky and fog colours, with the sun in it
// Everything is linear HDR (as in the game) and is tone mapped to sRGB at the end.
//
// Vertex inputs (rl_car.vsh): Color = light direction * 0.5 + 0.5 and its strength, UV1 = sky
// colour and moon flag (8 bits each), UV2 = the usual lightmap coordinates.

#include <minecraft:fog.glsl>
#include <minecraft:dynamictransforms.glsl>

layout(location = 0) in float sphericalVertexDistance;
layout(location = 1) in float cylindricalVertexDistance;
layout(location = 2) in vec3 worldPos;
layout(location = 3) in vec3 worldNormal;
layout(location = 4) in vec2 texCoord0;
layout(location = 5) in vec4 lightDirStrength;
layout(location = 6) in vec4 skyColorMoon;
layout(location = 7) in vec4 lightMapColor;
layout(location = 8) in float skyLevel;

layout(location = 0) out vec4 fragColor;

// Brightness of the environment (linear). Rocket League's arenas are lit by a strong key light
// over a soft ambient; these give a car in the midday sun about the same contrast.
#define RL_SUN_COLOR vec3(1.0, 0.95, 0.88)
#define RL_SUN_INTENSITY 2.6
#define RL_MOON_COLOR vec3(0.55, 0.65, 1.0)
#define RL_MOON_INTENSITY 0.12
#define RL_AMBIENT 0.75
#define RL_SKY_LIGHT 0.35
#define RL_EXPOSURE 1.0
#define UU_PER_BLOCK 100.0

vec3 srgbToLinear(vec3 c) {
    return mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), c));
}

vec3 linearToSrgb(vec3 c) {
    c = max(c, vec3(0.0));
    return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, step(vec3(0.0031308), c));
}

struct RlEnv {
    vec3 N;            // geometric normal, Minecraft world axes
    vec3 T;            // tangent frame: T = +U, B = -V (Unreal's tangent space, DirectX normal maps)
    vec3 B;
    vec3 V;            // towards the camera
    vec3 L;            // towards the light
    vec3 lightColor;   // the directional light (the game's LightColor)
    vec3 zenith;       // sky colour, linear
    vec3 horizon;      // fog colour, linear
    vec3 ambient;      // lightmap colour at the car, linear
    float sky;         // sky light at the car, 0..1
    float day;         // how much daylight there is, 0..1
    float depthUu;     // view distance, Unreal units (the game's v7.w)
};

RlEnv rlEnv() {
    RlEnv e;
    e.N = normalize(worldNormal);
    if (!gl_FrontFacing) {
        e.N = -e.N;
    }
    // Tangent frame from screen-space derivatives (the vertex format has no tangents).
    vec3 dp1 = dFdx(worldPos);
    vec3 dp2 = dFdy(worldPos);
    vec2 duv1 = dFdx(texCoord0);
    vec2 duv2 = dFdy(texCoord0);
    vec3 dp2perp = cross(dp2, e.N);
    vec3 dp1perp = cross(e.N, dp1);
    vec3 t = dp2perp * duv1.x + dp1perp * duv2.x;
    vec3 b = dp2perp * duv1.y + dp1perp * duv2.y;
    t -= e.N * dot(e.N, t);
    b -= e.N * dot(e.N, b);
    e.T = dot(t, t) > 1e-20 ? normalize(t) : normalize(cross(e.N, abs(e.N.y) < 0.9 ? vec3(0, 1, 0) : vec3(1, 0, 0)));
    e.B = dot(b, b) > 1e-20 ? -normalize(b) : cross(e.T, e.N);
    e.V = normalize(-worldPos);
    e.L = normalize(lightDirStrength.xyz);
    e.sky = skyLevel;
    e.ambient = srgbToLinear(lightMapColor.rgb);
    e.zenith = srgbToLinear(skyColorMoon.rgb);
    e.horizon = srgbToLinear(FogColor.rgb);
    float strength = lightDirStrength.w;
    bool moon = skyColorMoon.w > 0.5;
    e.day = moon ? 0.0 : strength;
    e.lightColor = (moon ? RL_MOON_COLOR * RL_MOON_INTENSITY : RL_SUN_COLOR * RL_SUN_INTENSITY) * strength * e.sky * e.sky;
    e.depthUu = length(worldPos) * UU_PER_BLOCK;
    return e;
}

vec3 toTangent(RlEnv e, vec3 v) {
    return vec3(dot(v, e.T), dot(v, e.B), dot(v, e.N));
}

vec3 toWorld(RlEnv e, vec3 t) {
    return e.T * t.x + e.B * t.y + e.N * t.z;
}

// Minecraft world axes (Y up) -> Unreal world axes (Z up), as everywhere else in the mod.
vec3 ueAxes(vec3 v) {
    return vec3(v.x, v.z, v.y);
}

// The reflection cube: a sky dome over a dark ground, with the sun in it.
vec3 envCube(RlEnv e, vec3 r) {
    r = normalize(r);
    float h = r.y;
    vec3 sky = mix(e.horizon, e.zenith, smoothstep(0.0, 0.6, h)) * (0.15 + 1.1 * e.day);
    vec3 ground = mix(e.horizon * 0.45, vec3(0.05, 0.048, 0.045) * (0.2 + e.day), smoothstep(0.0, -0.35, h));
    vec3 env = h > 0.0 ? sky : ground;
    env += e.lightColor * (pow(max(dot(r, e.L), 0.0), 400.0) * 6.0 + pow(max(dot(r, e.L), 0.0), 12.0) * 0.12);
    // Under a roof the sky is not visible: reflect the light around the car instead.
    return mix(e.ambient * 0.5, env, e.sky);
}

// The game's spherical-harmonic ambient (irradiance for a world normal).
vec3 shIrradiance(RlEnv e, vec3 n) {
    float up = clamp(n.y * 0.5 + 0.5, 0.0, 1.0);
    return e.ambient * RL_AMBIENT * mix(0.45, 1.0, up);
}

vec3 skyUpper(RlEnv e) {
    return mix(e.zenith, e.horizon, 0.3) * RL_SKY_LIGHT * e.sky * (0.25 + 0.75 * e.day);
}

vec3 skyLower(RlEnv e) {
    return vec3(0.09, 0.085, 0.075) * RL_SKY_LIGHT * e.sky * (0.25 + 0.75 * e.day);
}

vec3 ambientColor(RlEnv e) {
    return e.ambient * 0.04;
}

// The game's base pass output (o0) to the screen: exposure, filmic tone curve, sRGB, fog.
vec4 rlFinish(vec3 hdr) {
    vec3 x = max(hdr * RL_EXPOSURE, vec3(0.0));
    // Filmic shoulder (ACES approximation, Narkowicz) on the brightest channel, applied to all
    // three so that saturated paints keep their hue (orange stays orange in the sun).
    float peak = max(max(x.r, x.g), max(x.b, 1e-6));
    float curve = clamp((peak * (2.51 * peak + 0.03)) / (peak * (2.43 * peak + 0.59) + 0.14), 0.0, 1.0);
    vec3 mapped = x * (curve / peak);
    vec4 color = vec4(linearToSrgb(mapped), 1.0) * ColorModulator;
    return apply_fog(color, sphericalVertexDistance, cylindricalVertexDistance, FogEnvironmentalStart, FogEnvironmentalEnd, FogRenderDistanceStart, FogRenderDistanceEnd, FogColor);
}

// Sky light term shared by every material: albedo lit by the upper and lower sky.
vec3 skyLight(RlEnv e, vec3 albedo, float upDot) {
    vec2 w = vec2(0.5 + 0.5 * upDot, 0.5 - 0.5 * upDot);
    w *= w;
    return albedo * w.x * skyUpper(e) + albedo * w.y * skyLower(e);
}

#endif

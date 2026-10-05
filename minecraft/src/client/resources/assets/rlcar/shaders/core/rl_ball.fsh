#version 330
#extension GL_ARB_separate_shader_objects : require

// Rocket League's ball (MAT_Ball_V3), translated instruction by instruction from the game's
// compiled pixel shader (RefShaderCache-PC-D3D-SM5, base pass with the SH + directional light
// policy and sky light; disassembled with tools/rl_assets/shader_cache.py).
//
// What it does: the diffuse with its normal map plus the tiled Detail_Matte normal; a soft rim
// that brightens towards the edges; a reflection tinted by the team colour of the field half the
// ball is in (the TeamColor_WorldSpace material function: a warm white within 1024 uu of the
// centre line, then blue or orange); and the light strips (Mask.r), lit in that team colour and
// pulsing once a second (the last eighth of every second ramps them from 1/16 to 4 + 1/16).
//
// The game also brightens the ball as it nears a goal (GoalPlaneLocation / GoalPlaneNormal,
// GoalProximity, set from native code); there are no goals here, so those keep the material's
// defaults, which switch them off. Its reflection cube is parallax corrected to the arena's box
// (+-5000, +-7500, +-5000 uu); here the environment is the sky dome, looked up by direction.
//
// Defines: P36 blue team colour, P38 orange team colour (the engine constants the function reads).
// The ball's position along the field (RL Y / 1024 uu from the kickoff spot, clamped to -1..1)
// comes from rl_car.vsh (RL_BALL).

#include <rlcar:rl_env.glsl>
#include <minecraft:globals.glsl>

uniform sampler2D NormalMap;   // t1 Ball_Default00_N
uniform sampler2D DetailMap;   // t2 Detail_Matte.Matte_N, tiled 8 times
uniform sampler2D MaskMap;     // t3 Ball_Default00_RGB (R: light strips, G: reflection strength)
uniform sampler2D DiffuseMap;  // t4 Ball_Default00_D (sRGB)

layout(location = 9) in float fieldY;

#define BLUE vec3(P36_X, P36_Y, P36_Z)
#define ORANGE vec3(P38_X, P38_Y, P38_Z)
// TeamColor_WorldSpace's colour on the centre line.
#define NEUTRAL vec3(1.0, 0.968664, 0.906081)
// The goal parameters' defaults: GoalPlaneLocation (0, 0, -1024), GoalPlaneNormal (0, 0, 1) and
// GoalProximity 0. The plane is below the field, so the ball is never past it (g = 0).
#define GOAL_PROXIMITY 0.0

void main() {
    RlEnv e = rlEnv();
    vec2 uv = texCoord0;
    vec3 V = normalize(toTangent(e, e.V));
    vec3 L = normalize(toTangent(e, e.L));
    vec3 sky = normalize(toTangent(e, vec3(0.0, 1.0, 0.0)));
    float time = GameTime * 1200.0; // seconds

    // Normal: the ball's normal map plus half of the detail normal's slope.
    vec3 n1 = texture(NormalMap, uv).xyz * 2.0 - 1.0;
    vec3 d = texture(DetailMap, uv * 8.0).xyz * 2.0 - 1.0;
    vec3 nRaw = vec3(d.xy * 0.5, 0.0) + n1;
    vec3 n = normalize(nRaw);
    vec3 R = n * dot(n, V) * 2.0 - V;

    // Base: the diffuse, brighter with distance (x0.5 close up .. x1.5 from 4096 uu).
    float g = 0.0;
    vec3 diffuse = srgbToLinear(texture(DiffuseMap, uv).rgb);
    vec3 D = diffuse * (1.0 - g) * (clamp(e.depthUu * 0.000244, 0.0, 1.0) + 0.5);
    float ndv = dot(nRaw, V);
    vec3 base = D * ndv;
    float edge = 1.0 - clamp((ndv - 0.0625) * 2.0, 0.0, 1.0);
    edge = edge * edge * 0.25;
    base = edge * ((D * ndv + 0.0625) * 16.0 - base) + base;

    // TeamColor_WorldSpace: blue on the -Y half, orange on the +Y half, from 1024 uu out.
    float y = fieldY * 1024.0;
    vec3 team = mix(BLUE, ORANGE, clamp((y * 0.000976 + 1.0) * 0.5, 0.0, 1.0));
    vec3 tc = min(abs(y * 0.000976), 1.0) * (team * 2.0 - NEUTRAL) + NEUTRAL;

    // Reflection, tinted by the team colour; the strips glow in it.
    vec3 cube = envCube(e, toWorld(e, R));
    vec3 env = cube * tc + cube;
    vec4 mask = texture(MaskMap, uv);
    float k = clamp((mask.y - 0.125) * 8.0, 0.0, 1.0) * 0.75 + 2.0;
    vec3 glow = tc * mask.x * 64.0;
    vec3 C = env * mix(D * k, vec3(1.0), clamp(glow, 0.0, 1.0)) + base;

    // Emissive: the strips' pulse, and the goal flash (off here).
    float pulse = clamp((fract(time) - 0.875) * 8.0, 0.0, 1.0) * 4.0 + 0.0625;
    float facing = clamp(ndv - 0.5, 0.0, 1.0);
    facing *= facing;
    float flash = (g * ((1.0 - ndv) * (1.0 - ndv) - facing) + facing) * fract(time * 8.0) * GOAL_PROXIMITY;
    flash = flash * 8.0 + (1.0 - abs((g - 0.5) * 2.0)) * 96.0;
    vec3 emissive = diffuse * flash + pulse * glow;

    vec3 H = normalize(L + V);
    float ndh = clamp(dot(n, H), 0.0, 1.0);
    ndh = ndh < 0.000001 ? 0.0 : ndh;

    vec3 color = skyLight(e, C, dot(sky, n)) + emissive;
    color = C * max(shIrradiance(e, toWorld(e, n)), 0.0) + color;
    color = C * ndh * e.lightColor + color;
    color = C * ambientColor(e) + color;
#if defined(RL_DEBUG_1)
    fragColor = rlFinish(C);
#elif defined(RL_DEBUG_2)
    fragColor = rlFinish(C * ndh * e.lightColor);
#elif defined(RL_DEBUG_4)
    fragColor = rlFinish(emissive);
#else
    fragColor = rlFinish(color);
#endif
}

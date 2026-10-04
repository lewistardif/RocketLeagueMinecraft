#pragma once
#include "invoker.h"

namespace AUDIO {
inline void SET_VEHICLE_BOOST_ACTIVE(Vehicle vehicle, BOOL toggle) { return invoke<void>(0x4A04DE7CAB2739A1ULL, vehicle, toggle); }
inline void SET_VEHICLE_RADIO_ENABLED(Vehicle vehicle, BOOL toggle) { return invoke<void>(0x3B988190C0AA6C0BULL, vehicle, toggle); }
inline BOOL REQUEST_SCRIPT_AUDIO_BANK(const char* audioBank, BOOL p1, Any p2) { return invoke<BOOL>(0x2F844A8B08D76685ULL, audioBank, p1, p2); }
inline void PLAY_SOUND_FROM_ENTITY(int soundId, const char* audioName, Entity entity, const char* audioRef, BOOL isNetwork, Any p5) { return invoke<void>(0xE65F427EB70AB1EDULL, soundId, audioName, entity, audioRef, isNetwork, p5); }
inline void PLAY_SOUND_FRONTEND(int soundId, const char* audioName, const char* audioRef, BOOL p3) { return invoke<void>(0x67C540AA08E4A6F5ULL, soundId, audioName, audioRef, p3); }
inline void STOP_SOUND(int soundId) { return invoke<void>(0xA3B0C41BA5CC0BB5ULL, soundId); }
inline int GET_SOUND_ID() { return invoke<int>(0x430386FE9BF80B45ULL); }
inline void RELEASE_SOUND_ID(int soundId) { return invoke<void>(0x353FC880830B88FAULL, soundId); }
}

namespace CAMERA {
inline Cam CREATE_CAM(const char* camName, BOOL p1) { return invoke<Cam>(0xC3981DCE61D9E13FULL, camName, p1); }
inline void DESTROY_CAM(Cam cam, BOOL bScriptHostCam) { return invoke<void>(0x865908C81A2C22E9ULL, cam, bScriptHostCam); }
inline void SET_CAM_ACTIVE(Cam cam, BOOL active) { return invoke<void>(0x026FB97D0A425F84ULL, cam, active); }
inline void RENDER_SCRIPT_CAMS(BOOL render, BOOL ease, int easeTime, BOOL p3, BOOL p4, Any p5) { return invoke<void>(0x07E5B515DB0636FCULL, render, ease, easeTime, p3, p4, p5); }
inline void SET_CAM_COORD(Cam cam, float posX, float posY, float posZ) { return invoke<void>(0x4D41783FB745E42EULL, cam, posX, posY, posZ); }
inline void SET_CAM_ROT(Cam cam, float rotX, float rotY, float rotZ, int rotationOrder) { return invoke<void>(0x85973643155D0B07ULL, cam, rotX, rotY, rotZ, rotationOrder); }
inline void SET_CAM_FOV(Cam cam, float fieldOfView) { return invoke<void>(0xB13C14F66A00D047ULL, cam, fieldOfView); }
inline void SET_CAM_NEAR_CLIP(Cam cam, float nearClip) { return invoke<void>(0xC7848EFCCC545182ULL, cam, nearClip); }
inline void SET_CAM_FAR_CLIP(Cam cam, float farClip) { return invoke<void>(0xAE306F2A904BF86EULL, cam, farClip); }
inline Vector3 GET_GAMEPLAY_CAM_ROT(int rotationOrder) { return invoke<Vector3>(0x837765A25378F0BBULL, rotationOrder); }
inline Vector3 GET_GAMEPLAY_CAM_COORD() { return invoke<Vector3>(0x14D6F5678D8F1B37ULL); }
inline void SET_GAMEPLAY_CAM_RELATIVE_HEADING(float heading) { return invoke<void>(0xB4EC2312F4E5B1F1ULL, heading); }
inline void INVALIDATE_IDLE_CAM() { return invoke<void>(0xF4F2C0D4EE209E20ULL); }
inline void SHAKE_GAMEPLAY_CAM(const char* shakeName, float intensity) { return invoke<void>(0xFD55E49555E017CFULL, shakeName, intensity); }
inline void SHAKE_CAM(Cam cam, const char* type, float amplitude) { return invoke<void>(0x6A25241C340D3822ULL, cam, type, amplitude); }
inline float GET_GAMEPLAY_CAM_FOV() { return invoke<float>(0x65019750A0324133ULL); }
inline Vector3 GET_FINAL_RENDERED_CAM_COORD() { return invoke<Vector3>(0xA200EB1EE790F448ULL); }
inline Vector3 GET_FINAL_RENDERED_CAM_ROT(int rotationOrder) { return invoke<Vector3>(0x5B4E4C817FCC2DFBULL, rotationOrder); }
}

namespace DLC {
inline BOOL GET_IS_LOADING_SCREEN_ACTIVE() { return invoke<BOOL>(0x10D0A8F259E93EC9ULL); }
}

namespace ENTITY {
inline Vector3 GET_ENTITY_COORDS(Entity entity, BOOL alive) { return invoke<Vector3>(0x3FEF770D40960D5AULL, entity, alive); }
inline float GET_ENTITY_HEADING(Entity entity) { return invoke<float>(0xE83D4F9BA2A38914ULL, entity); }
inline void SET_ENTITY_COORDS_NO_OFFSET(Entity entity, float xPos, float yPos, float zPos, BOOL xAxis, BOOL yAxis, BOOL zAxis) { return invoke<void>(0x239A3351AC1DA385ULL, entity, xPos, yPos, zPos, xAxis, yAxis, zAxis); }
inline void SET_ENTITY_COORDS(Entity entity, float xPos, float yPos, float zPos, BOOL xAxis, BOOL yAxis, BOOL zAxis, BOOL clearArea) { return invoke<void>(0x06843DA7060A026BULL, entity, xPos, yPos, zPos, xAxis, yAxis, zAxis, clearArea); }
inline void SET_ENTITY_HEADING(Entity entity, float heading) { return invoke<void>(0x8E2530AA8ADA980EULL, entity, heading); }
inline void SET_ENTITY_QUATERNION(Entity entity, float x, float y, float z, float w) { return invoke<void>(0x77B21BE7AC540F07ULL, entity, x, y, z, w); }
inline void FREEZE_ENTITY_POSITION(Entity entity, BOOL toggle) { return invoke<void>(0x428CA6DBD1094446ULL, entity, toggle); }
inline void SET_ENTITY_COLLISION(Entity entity, BOOL toggle, BOOL keepPhysics) { return invoke<void>(0x1A9205C1B9EE827FULL, entity, toggle, keepPhysics); }
inline void SET_ENTITY_HAS_GRAVITY(Entity entity, BOOL toggle) { return invoke<void>(0x4A4722448F18EEF5ULL, entity, toggle); }
inline void SET_ENTITY_VELOCITY(Entity entity, float x, float y, float z) { return invoke<void>(0x1C99BB7B6E96D16FULL, entity, x, y, z); }
inline Vector3 GET_ENTITY_VELOCITY(Entity entity) { return invoke<Vector3>(0x4805D2B1D8CF94A9ULL, entity); }
inline void SET_ENTITY_INVINCIBLE(Entity entity, BOOL toggle, BOOL dontResetOnCleanup) { return invoke<void>(0x3882114BDE571AD4ULL, entity, toggle, dontResetOnCleanup); }
inline BOOL DOES_ENTITY_EXIST(Entity entity) { return invoke<BOOL>(0x7239B21A38F536BAULL, entity); }
inline void DELETE_ENTITY(Entity* entity) { return invoke<void>(0xAE3CBE5BF394C9C9ULL, entity); }
inline void SET_ENTITY_AS_MISSION_ENTITY(Entity entity, BOOL bScriptHostObject, BOOL bGrabFromOtherScript) { return invoke<void>(0xAD738C3085FE7E11ULL, entity, bScriptHostObject, bGrabFromOtherScript); }
inline void SET_ENTITY_VISIBLE(Entity entity, BOOL toggle, BOOL p2) { return invoke<void>(0xEA1C610A04DB6BBBULL, entity, toggle, p2); }
inline void SET_ENTITY_ALPHA(Entity entity, int alphaLevel, BOOL skin) { return invoke<void>(0x44A0870B7E92D7C0ULL, entity, alphaLevel, skin); }
inline void RESET_ENTITY_ALPHA(Entity entity) { return invoke<void>(0x9B1E824FFBB7027AULL, entity); }
inline Hash GET_ENTITY_MODEL(Entity entity) { return invoke<Hash>(0x9F47B058362C84B5ULL, entity); }
inline void APPLY_FORCE_TO_ENTITY(Entity entity, int forceFlags, float x, float y, float z, float offX, float offY, float offZ, int boneIndex, BOOL isDirectionRel, BOOL ignoreUpVec, BOOL isForceRel, BOOL p12, BOOL p13) { return invoke<void>(0xC5F68BE9613E2D18ULL, entity, forceFlags, x, y, z, offX, offY, offZ, boneIndex, isDirectionRel, ignoreUpVec, isForceRel, p12, p13); }
inline void APPLY_FORCE_TO_ENTITY_CENTER_OF_MASS(Entity entity, int forceType, float x, float y, float z, BOOL p5, BOOL isDirectionRel, BOOL isForceRel, BOOL p8) { return invoke<void>(0x18FF00FC7EFF559EULL, entity, forceType, x, y, z, p5, isDirectionRel, isForceRel, p8); }
inline BOOL IS_ENTITY_A_VEHICLE(Entity entity) { return invoke<BOOL>(0x6AC7003FA6E5575EULL, entity); }
inline BOOL IS_ENTITY_A_PED(Entity entity) { return invoke<BOOL>(0x524AC5ECEA15343EULL, entity); }
inline BOOL IS_ENTITY_AN_OBJECT(Entity entity) { return invoke<BOOL>(0x8D68C8FD0FACA94EULL, entity); }
inline void SET_ENTITY_NO_COLLISION_ENTITY(Entity entity1, Entity entity2, BOOL thisFrameOnly) { return invoke<void>(0xA53ED5520C07654AULL, entity1, entity2, thisFrameOnly); }
inline Vector3 GET_ENTITY_FORWARD_VECTOR(Entity entity) { return invoke<Vector3>(0x0A794A5A57F8DF91ULL, entity); }
inline float GET_ENTITY_SPEED(Entity entity) { return invoke<float>(0xD5037BA82E12416FULL, entity); }
inline BOOL IS_ENTITY_DEAD(Entity entity, BOOL p1) { return invoke<BOOL>(0x5F9532F3B5CC2551ULL, entity, p1); }
inline void SET_ENTITY_DYNAMIC(Entity entity, BOOL toggle) { return invoke<void>(0x1718DE8E3F2823CAULL, entity, toggle); }
inline void SET_ENTITY_LOAD_COLLISION_FLAG(Entity entity, BOOL toggle, Any p2) { return invoke<void>(0x0DC7CABAB1E9B67EULL, entity, toggle, p2); }
inline void GET_ENTITY_MATRIX(Entity entity, Vector3* forwardVector, Vector3* rightVector, Vector3* upVector, Vector3* position) { return invoke<void>(0xECB2FC7235A7D137ULL, entity, forwardVector, rightVector, upVector, position); }
inline void SET_ENTITY_HEALTH(Entity entity, int health, Entity instigator, Hash weaponType) { return invoke<void>(0x6B76DC1F3AE6E6A3ULL, entity, health, instigator, weaponType); }
inline void SET_ENTITY_PROOFS(Entity entity, BOOL bulletProof, BOOL fireProof, BOOL explosionProof, BOOL collisionProof, BOOL meleeProof, BOOL steamProof, BOOL dontResetOnCleanup, BOOL waterProof) { return invoke<void>(0xFAEE099C6F890BB8ULL, entity, bulletProof, fireProof, explosionProof, collisionProof, meleeProof, steamProof, dontResetOnCleanup, waterProof); }
inline void SET_ENTITY_CAN_BE_DAMAGED(Entity entity, BOOL toggle) { return invoke<void>(0x1760FFA8AB074D66ULL, entity, toggle); }
inline Vector3 GET_OFFSET_FROM_ENTITY_IN_WORLD_COORDS(Entity entity, float offsetX, float offsetY, float offsetZ) { return invoke<Vector3>(0x1899F328B0E12848ULL, entity, offsetX, offsetY, offsetZ); }
inline void SET_ENTITY_AS_NO_LONGER_NEEDED(Entity* entity) { return invoke<void>(0xB736A491E64A32CFULL, entity); }
}

namespace FIRE {
inline void ADD_EXPLOSION(float x, float y, float z, int explosionType, float damageScale, BOOL isAudible, BOOL isInvisible, float cameraShake, BOOL noDamage) { return invoke<void>(0xE3AD2BDBAEE269ACULL, x, y, z, explosionType, damageScale, isAudible, isInvisible, cameraShake, noDamage); }
inline void ADD_OWNED_EXPLOSION(Ped ped, float x, float y, float z, int explosionType, float damageScale, BOOL isAudible, BOOL isInvisible, float cameraShake) { return invoke<void>(0x172AA1B624FA1013ULL, ped, x, y, z, explosionType, damageScale, isAudible, isInvisible, cameraShake); }
}

namespace GRAPHICS {
inline void USE_PARTICLE_FX_ASSET(const char* name) { return invoke<void>(0x6C38AF3693A69A91ULL, name); }
inline int START_PARTICLE_FX_LOOPED_ON_ENTITY(const char* effectName, Entity entity, float xOffset, float yOffset, float zOffset, float xRot, float yRot, float zRot, float scale, BOOL xAxis, BOOL yAxis, BOOL zAxis) { return invoke<int>(0x1AE42C1660FD6517ULL, effectName, entity, xOffset, yOffset, zOffset, xRot, yRot, zRot, scale, xAxis, yAxis, zAxis); }
inline void STOP_PARTICLE_FX_LOOPED(int ptfxHandle, BOOL p1) { return invoke<void>(0x8F75998877616996ULL, ptfxHandle, p1); }
inline void SET_PARTICLE_FX_LOOPED_SCALE(int ptfxHandle, float scale) { return invoke<void>(0xB44250AAA456492DULL, ptfxHandle, scale); }
inline void SET_PARTICLE_FX_LOOPED_ALPHA(int ptfxHandle, float alpha) { return invoke<void>(0x726845132380142EULL, ptfxHandle, alpha); }
inline void DRAW_LIGHT_WITH_RANGE(float posX, float posY, float posZ, int colorR, int colorG, int colorB, float range, float intensity) { return invoke<void>(0xF2A1B2771A01DBD4ULL, posX, posY, posZ, colorR, colorG, colorB, range, intensity); }
inline BOOL START_PARTICLE_FX_NON_LOOPED_ON_ENTITY(const char* effectName, Entity entity, float offsetX, float offsetY, float offsetZ, float rotX, float rotY, float rotZ, float scale, BOOL axisX, BOOL axisY, BOOL axisZ) { return invoke<BOOL>(0x0D53A3B8DA0809D2ULL, effectName, entity, offsetX, offsetY, offsetZ, rotX, rotY, rotZ, scale, axisX, axisY, axisZ); }
inline void DRAW_RECT(float x, float y, float width, float height, int r, int g, int b, int a, BOOL p8) { return invoke<void>(0x3A618A217E5154F0ULL, x, y, width, height, r, g, b, a, p8); }
inline void DRAW_LINE(float x1, float y1, float z1, float x2, float y2, float z2, int red, int green, int blue, int alpha) { return invoke<void>(0x6B7256074AE34680ULL, x1, y1, z1, x2, y2, z2, red, green, blue, alpha); }
inline void DRAW_MARKER(int type, float posX, float posY, float posZ, float dirX, float dirY, float dirZ, float rotX, float rotY, float rotZ, float scaleX, float scaleY, float scaleZ, int red, int green, int blue, int alpha, BOOL bobUpAndDown, BOOL faceCamera, int rotationOrder, BOOL rotate, const char* textureDict, const char* textureName, BOOL invert) { return invoke<void>(0x28477EC23D892089ULL, type, posX, posY, posZ, dirX, dirY, dirZ, rotX, rotY, rotZ, scaleX, scaleY, scaleZ, red, green, blue, alpha, bobUpAndDown, faceCamera, rotationOrder, rotate, textureDict, textureName, invert); }
inline int START_PARTICLE_FX_LOOPED_AT_COORD(const char* effectName, float x, float y, float z, float xRot, float yRot, float zRot, float scale, BOOL xAxis, BOOL yAxis, BOOL zAxis, BOOL p11) { return invoke<int>(0xE184F4F0DC5910E7ULL, effectName, x, y, z, xRot, yRot, zRot, scale, xAxis, yAxis, zAxis, p11); }
inline void REMOVE_PARTICLE_FX(int ptfxHandle, BOOL p1) { return invoke<void>(0xC401503DFE8D53CFULL, ptfxHandle, p1); }
inline void SET_PARTICLE_FX_LOOPED_OFFSETS(int ptfxHandle, float x, float y, float z, float rotX, float rotY, float rotZ) { return invoke<void>(0xF7DDEBEC43483C43ULL, ptfxHandle, x, y, z, rotX, rotY, rotZ); }
inline void SET_PARTICLE_FX_LOOPED_EVOLUTION(int ptfxHandle, const char* propertyName, float amount, BOOL noNetwork) { return invoke<void>(0x5F0C4B5B1C393BE2ULL, ptfxHandle, propertyName, amount, noNetwork); }
inline void SET_PARTICLE_FX_LOOPED_COLOUR(int ptfxHandle, float r, float g, float b, BOOL p4) { return invoke<void>(0x7F8F65877F88783BULL, ptfxHandle, r, g, b, p4); }
inline BOOL START_PARTICLE_FX_NON_LOOPED_AT_COORD(const char* effectName, float xPos, float yPos, float zPos, float xRot, float yRot, float zRot, float scale, BOOL xAxis, BOOL yAxis, BOOL zAxis) { return invoke<BOOL>(0x25129531F77B9ED3ULL, effectName, xPos, yPos, zPos, xRot, yRot, zRot, scale, xAxis, yAxis, zAxis); }
inline BOOL DOES_PARTICLE_FX_LOOPED_EXIST(int ptfxHandle) { return invoke<BOOL>(0x74AFEF0D2E1E409BULL, ptfxHandle); }
inline void DRAW_POLY(float x1, float y1, float z1, float x2, float y2, float z2, float x3, float y3, float z3, int red, int green, int blue, int alpha) { return invoke<void>(0xAC26716048436851ULL, x1, y1, z1, x2, y2, z2, x3, y3, z3, red, green, blue, alpha); }
}

namespace HUD {
inline void SET_TEXT_FONT(int fontType) { return invoke<void>(0x66E0276CC5F6B9DAULL, fontType); }
inline void SET_TEXT_SCALE(float scale, float size) { return invoke<void>(0x07C837F9A01C34C9ULL, scale, size); }
inline void SET_TEXT_COLOUR(int red, int green, int blue, int alpha) { return invoke<void>(0xBE6B23FFA53FB442ULL, red, green, blue, alpha); }
inline void SET_TEXT_OUTLINE() { return invoke<void>(0x2513DFB0FB8400FEULL); }
inline void SET_TEXT_CENTRE(BOOL align) { return invoke<void>(0xC02F4DBFB51D988BULL, align); }
inline void SET_TEXT_DROPSHADOW(int distance, int r, int g, int b, int a) { return invoke<void>(0x465C84BC39F1C351ULL, distance, r, g, b, a); }
inline void BEGIN_TEXT_COMMAND_DISPLAY_TEXT(const char* text) { return invoke<void>(0x25FBB336DF1804CBULL, text); }
inline void ADD_TEXT_COMPONENT_SUBSTRING_PLAYER_NAME(const char* text) { return invoke<void>(0x6C188BE134E074AAULL, text); }
inline void END_TEXT_COMMAND_DISPLAY_TEXT(float x, float y, int p2) { return invoke<void>(0xCD015E5BB0D96A57ULL, x, y, p2); }
inline void BEGIN_TEXT_COMMAND_THEFEED_POST(const char* text) { return invoke<void>(0x202709F4C58A0424ULL, text); }
inline int END_TEXT_COMMAND_THEFEED_POST_MESSAGETEXT(const char* txdName, const char* textureName, BOOL flash, int iconType, const char* sender, const char* subject) { return invoke<int>(0x1CCD9A37359072CFULL, txdName, textureName, flash, iconType, sender, subject); }
inline int END_TEXT_COMMAND_THEFEED_POST_TICKER(BOOL blink, BOOL p1) { return invoke<int>(0x2ED7843F8F801023ULL, blink, p1); }
inline void BEGIN_TEXT_COMMAND_DISPLAY_HELP(const char* inputType) { return invoke<void>(0x8509B634FBE7DA11ULL, inputType); }
inline void END_TEXT_COMMAND_DISPLAY_HELP(int p0, BOOL loop, BOOL beep, int shape) { return invoke<void>(0x238FFE5C7B0498A6ULL, p0, loop, beep, shape); }
inline Blip ADD_BLIP_FOR_ENTITY(Entity entity) { return invoke<Blip>(0x5CDE92C702A8FCE7ULL, entity); }
inline void SET_BLIP_SPRITE(Blip blip, int spriteId) { return invoke<void>(0xDF735600A4696DAFULL, blip, spriteId); }
inline void SET_BLIP_COLOUR(Blip blip, int color) { return invoke<void>(0x03D7FB09E75D6B7EULL, blip, color); }
inline void REMOVE_BLIP(Blip* blip) { return invoke<void>(0x86A652570E5F25DDULL, blip); }
inline BOOL DOES_BLIP_EXIST(Blip blip) { return invoke<BOOL>(0xA6DB27D19ECBB7DAULL, blip); }
inline void SET_BLIP_AS_SHORT_RANGE(Blip blip, BOOL toggle) { return invoke<void>(0xBE8BE4FE60E27B72ULL, blip, toggle); }
inline void HIDE_HUD_AND_RADAR_THIS_FRAME() { return invoke<void>(0x719FF505F097FD20ULL); }
inline BOOL IS_PAUSE_MENU_ACTIVE() { return invoke<BOOL>(0xB0034A223497FFCBULL); }
}

namespace MISC {
inline void GET_MODEL_DIMENSIONS(Hash modelHash, Vector3* minimum, Vector3* maximum) { return invoke<void>(0x03E8D3D5F549087AULL, modelHash, minimum, maximum); }
inline Hash GET_HASH_KEY(const char* string) { return invoke<Hash>(0xD24D37CC275948CCULL, string); }
inline float GET_FRAME_TIME() { return invoke<float>(0x15C40837039FFAF7ULL); }
inline int GET_GAME_TIMER() { return invoke<int>(0x9CD27B0045628463ULL); }
inline BOOL GET_GROUND_Z_FOR_3D_COORD(float x, float y, float z, float* groundZ, BOOL ignoreWater, BOOL p5) { return invoke<BOOL>(0xC906A7DAB05C8D2BULL, x, y, z, groundZ, ignoreWater, p5); }
inline void SHOOT_SINGLE_BULLET_BETWEEN_COORDS(float x1, float y1, float z1, float x2, float y2, float z2, int damage, BOOL p7, Hash weaponHash, Ped ownerPed, BOOL isAudible, BOOL isInvisible, float speed) { return invoke<void>(0x867654CBC7606F2CULL, x1, y1, z1, x2, y2, z2, damage, p7, weaponHash, ownerPed, isAudible, isInvisible, speed); }
inline void SHOOT_SINGLE_BULLET_BETWEEN_COORDS_IGNORE_ENTITY(float x1, float y1, float z1, float x2, float y2, float z2, int damage, BOOL p7, Hash weaponHash, Ped ownerPed, BOOL isAudible, BOOL isInvisible, float speed, Entity entity, Any p14) { return invoke<void>(0xE3A7742E0B7A2F8BULL, x1, y1, z1, x2, y2, z2, damage, p7, weaponHash, ownerPed, isAudible, isInvisible, speed, entity, p14); }
}

namespace NETWORK {
inline void SET_ENTITY_LOCALLY_INVISIBLE(Entity entity) { return invoke<void>(0xE135A9FF3F5D05D8ULL, entity); }
}

namespace OBJECT {
inline Object CREATE_OBJECT(Hash modelHash, float x, float y, float z, BOOL isNetwork, BOOL bScriptHostObj, BOOL dynamic) { return invoke<Object>(0x509D5878EB39E842ULL, modelHash, x, y, z, isNetwork, bScriptHostObj, dynamic); }
inline Object CREATE_OBJECT_NO_OFFSET(Hash modelHash, float x, float y, float z, BOOL isNetwork, BOOL bScriptHostObj, BOOL dynamic, Any p7) { return invoke<Object>(0x9A294B2138ABB884ULL, modelHash, x, y, z, isNetwork, bScriptHostObj, dynamic, p7); }
}

namespace PAD {
inline void DISABLE_CONTROL_ACTION(int control, int action, BOOL disableRelatedActions) { return invoke<void>(0xFE99B66D079CF6BCULL, control, action, disableRelatedActions); }
inline void ENABLE_CONTROL_ACTION(int control, int action, BOOL enableRelatedActions) { return invoke<void>(0x351220255D64C155ULL, control, action, enableRelatedActions); }
inline BOOL IS_DISABLED_CONTROL_JUST_PRESSED(int control, int action) { return invoke<BOOL>(0x91AEF906BCA88877ULL, control, action); }
inline BOOL IS_CONTROL_JUST_PRESSED(int control, int action) { return invoke<BOOL>(0x580417101DDB492FULL, control, action); }
inline float GET_DISABLED_CONTROL_NORMAL(int control, int action) { return invoke<float>(0x11E65974A982637CULL, control, action); }
}

namespace PATH {
inline BOOL GET_CLOSEST_VEHICLE_NODE_WITH_HEADING(float x, float y, float z, Vector3* outPosition, float* outHeading, int nodeType, float p6, float p7) { return invoke<BOOL>(0xFF071FB798B803B0ULL, x, y, z, outPosition, outHeading, nodeType, p6, p7); }
}

namespace PED {
inline BOOL IS_PED_IN_ANY_VEHICLE(Ped ped, BOOL atGetIn) { return invoke<BOOL>(0x997ABD671D25CA0BULL, ped, atGetIn); }
inline BOOL IS_PED_IN_VEHICLE(Ped ped, Vehicle vehicle, BOOL atGetIn) { return invoke<BOOL>(0xA3EE4A07279BB9DBULL, ped, vehicle, atGetIn); }
inline Vehicle GET_VEHICLE_PED_IS_IN(Ped ped, BOOL includeEntering) { return invoke<Vehicle>(0x9A9112A0FE9A4713ULL, ped, includeEntering); }
inline Vehicle GET_VEHICLE_PED_IS_TRYING_TO_ENTER(Ped ped) { return invoke<Vehicle>(0x814FA8BE5449445DULL, ped); }
inline void SET_PED_INTO_VEHICLE(Ped ped, Vehicle vehicle, int seatIndex) { return invoke<void>(0xF75B0D629E1C063DULL, ped, vehicle, seatIndex); }
inline BOOL SET_PED_TO_RAGDOLL(Ped ped, int time1, int time2, int ragdollType, BOOL p4, BOOL p5, BOOL p6) { return invoke<BOOL>(0xAE99FB955581844AULL, ped, time1, time2, ragdollType, p4, p5, p6); }
inline void APPLY_DAMAGE_TO_PED(Ped ped, int damageAmount, BOOL p2, Any p3, Hash weaponType) { return invoke<void>(0x697157CED63F18D4ULL, ped, damageAmount, p2, p3, weaponType); }
inline BOOL IS_PED_DEAD_OR_DYING(Ped ped, BOOL checkMeleeDeathFlags) { return invoke<BOOL>(0x3317DEDB88C95038ULL, ped, checkMeleeDeathFlags); }
inline void SET_PED_CAN_BE_KNOCKED_OFF_VEHICLE(Ped ped, int state) { return invoke<void>(0x7A6535691B477C48ULL, ped, state); }
inline void SET_PED_CONFIG_FLAG(Ped ped, int flagId, BOOL value) { return invoke<void>(0x1913FE4CBF41C463ULL, ped, flagId, value); }
inline BOOL IS_PED_A_PLAYER(Ped ped) { return invoke<BOOL>(0x12534C348C6CB68BULL, ped); }
inline void SET_PED_RESET_FLAG(Ped ped, int flagId, BOOL value) { return invoke<void>(0xC1E8A365BF3B29F2ULL, ped, flagId, value); }
inline BOOL GET_CLOSEST_PED(float x, float y, float z, float radius, BOOL p4, BOOL p5, Ped* outPed, BOOL p7, BOOL p8, int pedType) { return invoke<BOOL>(0xC33AB876A77F8164ULL, x, y, z, radius, p4, p5, outPed, p7, p8, pedType); }
}

namespace PLAYER {
inline Ped PLAYER_PED_ID() { return invoke<Ped>(0xD80958FC74E988A6ULL); }
inline Player PLAYER_ID() { return invoke<Player>(0x4F8644AF03D0E0D6ULL); }
inline Ped GET_PLAYER_PED(Player player) { return invoke<Ped>(0x43A66C31C68491C0ULL, player); }
inline BOOL IS_PLAYER_CONTROL_ON(Player player) { return invoke<BOOL>(0x49C32D60007AFA47ULL, player); }
inline void SET_PLAYER_CONTROL(Player player, BOOL bHasControl, int flags) { return invoke<void>(0x8D32347D6D4C40A2ULL, player, bHasControl, flags); }
}

namespace SHAPETEST {
inline int START_EXPENSIVE_SYNCHRONOUS_SHAPE_TEST_LOS_PROBE(float x1, float y1, float z1, float x2, float y2, float z2, int flags, Entity entity, int p8) { return invoke<int>(0x377906D8A31E5586ULL, x1, y1, z1, x2, y2, z2, flags, entity, p8); }
inline int GET_SHAPE_TEST_RESULT(int shapeTestHandle, BOOL* hit, Vector3* endCoords, Vector3* surfaceNormal, Entity* entityHit) { return invoke<int>(0x3D87450E15D98694ULL, shapeTestHandle, hit, endCoords, surfaceNormal, entityHit); }
inline int GET_SHAPE_TEST_RESULT_INCLUDING_MATERIAL(int shapeTestHandle, BOOL* hit, Vector3* endCoords, Vector3* surfaceNormal, Hash* materialHash, Entity* entityHit) { return invoke<int>(0x65287525D951F6BEULL, shapeTestHandle, hit, endCoords, surfaceNormal, materialHash, entityHit); }
}

namespace STREAMING {
inline void REQUEST_NAMED_PTFX_ASSET(const char* fxName) { return invoke<void>(0xB80D8756B4668AB6ULL, fxName); }
inline BOOL HAS_NAMED_PTFX_ASSET_LOADED(const char* fxName) { return invoke<BOOL>(0x8702416E512EC454ULL, fxName); }
inline void REQUEST_MODEL(Hash model) { return invoke<void>(0x963D27A58DF860ACULL, model); }
inline BOOL HAS_MODEL_LOADED(Hash model) { return invoke<BOOL>(0x98A4EB5D89A0C952ULL, model); }
inline void SET_MODEL_AS_NO_LONGER_NEEDED(Hash model) { return invoke<void>(0xE532F5D78798DAABULL, model); }
inline BOOL IS_MODEL_IN_CDIMAGE(Hash model) { return invoke<BOOL>(0x35B9E0803292B641ULL, model); }
inline BOOL IS_MODEL_A_VEHICLE(Hash model) { return invoke<BOOL>(0x19AAC8F07BFEC53EULL, model); }
inline BOOL IS_MODEL_VALID(Hash model) { return invoke<BOOL>(0xC0296A2EDF545E92ULL, model); }
inline void REQUEST_COLLISION_AT_COORD(float x, float y, float z) { return invoke<void>(0x07503F7948F491A7ULL, x, y, z); }
}

namespace TASK {
inline void TASK_LEAVE_VEHICLE(Ped ped, Vehicle vehicle, int flags) { return invoke<void>(0xD3DBCE61A490BE02ULL, ped, vehicle, flags); }
inline void TASK_ENTER_VEHICLE(Ped ped, Vehicle vehicle, int timeout, int seat, float speed, int flag, const char* overrideEntryClipsetName) { return invoke<void>(0xC20E50AA46D09CA8ULL, ped, vehicle, timeout, seat, speed, flag, overrideEntryClipsetName); }
inline void CLEAR_PED_TASKS_IMMEDIATELY(Ped ped) { return invoke<void>(0xAAA34F8A7CB32098ULL, ped); }
}

namespace VEHICLE {
inline Ped GET_PED_IN_VEHICLE_SEAT(Vehicle vehicle, int seatIndex, BOOL p2) { return invoke<Ped>(0xBB40DD2270B65366ULL, vehicle, seatIndex, p2); }
inline void SET_VEHICLE_DAMAGE(Vehicle vehicle, float xOffset, float yOffset, float zOffset, float damage, float radius, BOOL focusOnModel) { return invoke<void>(0xA1DD317EA8FD4F29ULL, vehicle, xOffset, yOffset, zOffset, damage, radius, focusOnModel); }
inline BOOL IS_VEHICLE_ON_ALL_WHEELS(Vehicle vehicle) { return invoke<BOOL>(0xB104CD1BABF302E2ULL, vehicle); }
inline Vehicle CREATE_VEHICLE(Hash modelHash, float x, float y, float z, float heading, BOOL isNetwork, BOOL bScriptHostVeh, BOOL p7) { return invoke<Vehicle>(0xAF35D0D2583051B0ULL, modelHash, x, y, z, heading, isNetwork, bScriptHostVeh, p7); }
inline void SET_VEHICLE_ENGINE_ON(Vehicle vehicle, BOOL value, BOOL instantly, BOOL disableAutoStart) { return invoke<void>(0x2497C4717C8B881EULL, vehicle, value, instantly, disableAutoStart); }
inline void EXPLODE_VEHICLE(Vehicle vehicle, BOOL isAudible, BOOL isInvisible) { return invoke<void>(0xBA71116ADF5B514CULL, vehicle, isAudible, isInvisible); }
inline void SET_VEHICLE_COLOURS(Vehicle vehicle, int colorPrimary, int colorSecondary) { return invoke<void>(0x4F1D4BE3A7F24601ULL, vehicle, colorPrimary, colorSecondary); }
inline void SET_VEHICLE_CUSTOM_PRIMARY_COLOUR(Vehicle vehicle, int r, int g, int b) { return invoke<void>(0x7141766F91D15BEAULL, vehicle, r, g, b); }
inline void SET_VEHICLE_CUSTOM_SECONDARY_COLOUR(Vehicle vehicle, int r, int g, int b) { return invoke<void>(0x36CED73BFED89754ULL, vehicle, r, g, b); }
inline void SET_VEHICLE_DOORS_LOCKED(Vehicle vehicle, int doorLockStatus) { return invoke<void>(0xB664292EAECF7FA6ULL, vehicle, doorLockStatus); }
inline void SET_VEHICLE_CAN_BE_VISIBLY_DAMAGED(Vehicle vehicle, BOOL state) { return invoke<void>(0x4C7028F78FFD3681ULL, vehicle, state); }
inline void SET_VEHICLE_FIXED(Vehicle vehicle) { return invoke<void>(0x115722B1B9C14C1CULL, vehicle); }
inline void SET_VEHICLE_HAS_STRONG_AXLES(Vehicle vehicle, BOOL toggle) { return invoke<void>(0x92F0CF722BC4202FULL, vehicle, toggle); }
inline void SET_VEHICLE_REDUCE_GRIP(Vehicle vehicle, BOOL toggle) { return invoke<void>(0x222FF6A823D122E2ULL, vehicle, toggle); }
inline void SET_VEHICLE_GRAVITY(Vehicle vehicle, BOOL toggle) { return invoke<void>(0x89F149B6131E57DAULL, vehicle, toggle); }
inline void SET_VEHICLE_LIGHTS(Vehicle vehicle, int state) { return invoke<void>(0x34E710FF01247C5AULL, vehicle, state); }
inline void SET_VEHICLE_EXPLODES_ON_HIGH_EXPLOSION_DAMAGE(Vehicle vehicle, BOOL toggle) { return invoke<void>(0x71B0892EC081D60AULL, vehicle, toggle); }
inline BOOL IS_VEHICLE_DRIVEABLE(Vehicle vehicle, BOOL isOnFireCheck) { return invoke<BOOL>(0x4C241E39B23DF959ULL, vehicle, isOnFireCheck); }
inline void SET_VEHICLE_DENSITY_MULTIPLIER_THIS_FRAME(float multiplier) { return invoke<void>(0x245A6883D966D537ULL, multiplier); }
inline Vehicle GET_CLOSEST_VEHICLE(float x, float y, float z, float radius, Hash modelHash, int flags) { return invoke<Vehicle>(0xF73EB622C4F1689BULL, x, y, z, radius, modelHash, flags); }
}

namespace WEAPON {
inline void REQUEST_WEAPON_ASSET(Hash weaponHash, int p1, int p2) { return invoke<void>(0x5443438F033E29C3ULL, weaponHash, p1, p2); }
inline BOOL HAS_WEAPON_ASSET_LOADED(Hash weaponHash) { return invoke<BOOL>(0x36E353271F0E90EEULL, weaponHash); }
}

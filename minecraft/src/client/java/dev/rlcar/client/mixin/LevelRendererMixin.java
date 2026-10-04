package dev.rlcar.client.mixin;

import com.mojang.blaze3d.framegraph.FrameGraphBuilder;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import dev.rlcar.client.LinearFx;
import net.minecraft.client.renderer.LevelRenderer;
import net.minecraft.client.renderer.LevelTargetBundle;
import net.minecraft.client.renderer.chunk.ChunkSectionsToRender;
import net.minecraft.client.renderer.feature.FeatureRenderDispatcher;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(LevelRenderer.class)
public abstract class LevelRendererMixin {
	@Shadow
	@Final
	private LevelTargetBundle targets;

	/**
	 * The car effects' linear pass ({@link LinearFx}), after the main pass (opaque and translucent
	 * world, features, and with improved transparency the OIT composite) and before the outlines.
	 */
	@Inject(method = "addMainPass", at = @At("TAIL"))
	private void rlcar$effectsPass(FrameGraphBuilder frame, FeatureRenderDispatcher.PreparedFrame featureFrame, GpuBufferSlice terrainFog,
		ChunkSectionsToRender chunkSectionsToRender, boolean consistentDepthRequired, CallbackInfo ci) {
		LinearFx.addPass(frame, this.targets, terrainFog);
	}
}

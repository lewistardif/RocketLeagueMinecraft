package dev.rlcar.client;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.blaze3d.vertex.VertexConsumer;
import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import dev.rlcar.RlCar;
import dev.rlcar.physics.CarPose;
import dev.rlcar.physics.NativeBoostMeter;
import dev.rlcar.physics.RlCarNative;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.navigation.ScreenRectangle;
import net.minecraft.client.gui.render.TextureSetup;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.renderer.state.gui.GuiElementRenderState;
import net.minecraft.client.renderer.texture.AbstractTexture;
import net.minecraft.resources.Identifier;
import org.jspecify.annotations.Nullable;

/**
 * The HUD while driving: Rocket League's boost meter in the bottom-right corner, and the speed.
 *
 * <p>With the extracted HUD ({@code hud/} in the extracted assets, written by
 * {@code tools/rl_assets/hud.py}) this is the game's own meter, as in the Bevy demo: its textures,
 * 101-frame fill timelines, text fields and fonts come from the game's HUD movie, and what it shows
 * (number, colours, flashes, the blinking label) comes from the Rust core's port of the meter's
 * ActionScript ({@link NativeBoostMeter}). Each layer of the Scaleform clip is one GUI element, drawn
 * in the clip's order with {@code assets/rlcar/shaders/core/boost_meter.*}, which apply the clip's
 * Flash colour transforms. The 3D tilt and perspective are projected here per vertex
 * ({@link #project}, a copy of {@code BoostMeterLayout::project}), perspective-correct on the GPU.
 * Minecraft's GUI blends in gamma space like Scaleform, so the result matches the game.
 *
 * <p>Without the extracted HUD a plain bar is drawn instead.
 */
public final class CarHud {
	// rl_car_core::boost_meter constants (the HUD movie and BoostMeterView's script).
	private static final float STAGE_W = 1120.0F;
	private static final float STAGE_H = 720.0F;
	private static final float STAGE_X = 1010.0F;
	private static final float STAGE_Y = 610.0F;
	private static final float ROTATION_X = (float) Math.toRadians(-2.0);
	private static final float ROTATION_Y = (float) Math.toRadians(15.0);
	private static final float FRONT_Z = -20.0F;
	private static final float FOCAL = 500.0F;
	private static final float CENTER_DX = -50.0F;
	private static final float CENTER_DY = -100.0F;
	private static final float[] BOOST_TEXT_COLOR = {255.0F, 255.0F, 255.0F};
	private static final float[] BACKGROUND_TEXT_COLOR = {51.0F, 51.0F, 51.0F};
	private static final float[] LABEL_TEXT_COLOR = {102.0F, 102.0F, 102.0F};
	private static final String LABEL_TEXT = "BOOST";

	// Which texture channel is the alpha (the shader's selector): bitmap, glyph, glow 6, glow 8.
	private static final int CHANNEL_BITMAP = 0;
	private static final int CHANNEL_GLYPH = 1;
	private static final int CHANNEL_GLOW_NORMAL = 2;
	private static final int CHANNEL_GLOW_MAX = 3;

	private static @Nullable RenderPipeline pipeline;
	private static @Nullable Meter meter;
	private static boolean meterLoaded;
	private static @Nullable NativeBoostMeter state;
	private static long lastNanos;
	private static final float[] FRAME = new float[RlCarNative.BOOST_METER_FLOATS];

	private CarHud() {
	}

	public static void registerPipeline() {
		pipeline = RenderPipelines.register(RenderPipeline.builder(RenderPipelines.GUI_TEXTURED_SNIPPET)
			.withLocation(RlCar.id("pipeline/boost_meter"))
			.withVertexShader(RlCar.id("core/boost_meter"))
			.withFragmentShader(RlCar.id("core/boost_meter"))
			.withVertexBinding(0, DefaultVertexFormat.ENTITY)
			.withPrimitiveTopology(PrimitiveTopology.QUADS)
			.withCull(false) // the clip's polygons wind either way
			.build());
	}

	public static void draw(GuiGraphicsExtractor g, DeltaTracker delta) {
		CarPose pose = ClientDriving.pose();
		Minecraft mc = Minecraft.getInstance();
		if (pose == null) {
			if (state != null) {
				state.close();
				state = null;
			}
			return;
		}
		int w = g.guiWidth();
		int h = g.guiHeight();
		Meter m = meter();
		if (m != null && pipeline != null) {
			drawMeter(g, m, pose.boost);
		} else {
			int barW = 80;
			int barH = 6;
			int x = w - barW - 10;
			int y = h - 30;
			int fill = Math.round(barW * Math.clamp(pose.boost / 100.0F, 0.0F, 1.0F));
			g.fill(x - 1, y - 1, x + barW + 1, y + barH + 1, 0xA0000000);
			g.fill(x, y, x + fill, y + barH, pose.has(RlCarNative.FLAG_BOOSTING) ? 0xFFFFC040 : 0xFFF08A1C);
			g.text(mc.font, "BOOST " + Math.round(pose.boost), x, y - 11, 0xFFFFFFFF, true);
		}
		// uu/s -> km/h (1 uu = 1 cm).
		int kmh = Math.round(Math.abs(pose.forwardSpeed) * 0.036F);
		String speed = kmh + " km/h" + (pose.has(RlCarNative.FLAG_SUPERSONIC) ? "  SUPERSONIC" : "");
		g.text(mc.font, speed, 10, h - 20, 0xFFFFFFFF, true);
	}

	// ----------------------------------------------------------------------------- meter

	private static void drawMeter(GuiGraphicsExtractor g, Meter m, float boost) {
		long now = System.nanoTime();
		if (state == null) {
			state = new NativeBoostMeter();
			lastNanos = now;
		}
		float dt = Math.min((now - lastNanos) / 1.0e9F, 0.25F);
		lastNanos = now;
		if (!state.update(boost, dt, FRAME)) {
			return;
		}
		Minecraft mc = Minecraft.getInstance();
		float pxW = mc.getWindow().getWidth();
		float pxH = mc.getWindow().getHeight();
		float guiScale = (float) mc.getWindow().getGuiScale();
		Layout layout = Layout.of(pxW, pxH, guiScale);

		int frame = Math.clamp((int) FRAME[0], 1, m.fill.size());
		boolean maxGlow = FRAME[1] > 6.0F;
		float glowScale = FRAME[2];
		int len = (int) FRAME[3];
		StringBuilder sb = new StringBuilder();
		for (int i = 0; i < len; i++) {
			sb.append((char) FRAME[4 + i]);
		}
		String text = sb.toString();

		// The clip's layers in depth order.
		submit(g, layout, m.background, m.backgroundTris, 0.0F, 1.0F, ct(0), CHANNEL_BITMAP);
		submit(g, layout, m.glow, m.glowTris, 0.0F, glowScale, ct(1), CHANNEL_BITMAP);
		submit(g, layout, m.fillTintedTexture, m.fillTintedFrames.get(frame - 1), FRONT_Z, 1.0F, ct(3), CHANNEL_BITMAP);
		submit(g, layout, m.fillTexture, m.fill.get(frame - 1), FRONT_Z, 1.0F, ct(2), CHANNEL_BITMAP);
		submitText(g, layout, m, m.backgroundText, m.numbers, text, 0.0F, ct(4), BACKGROUND_TEXT_COLOR, -1);
		submitText(g, layout, m, m.label, m.header, LABEL_TEXT, 0.0F, ct(5), LABEL_TEXT_COLOR, -1);
		submitText(g, layout, m, m.boostText, m.numbers, text, FRONT_Z, ct(6), BOOST_TEXT_COLOR, maxGlow ? CHANNEL_GLOW_MAX : CHANNEL_GLOW_NORMAL);
	}

	/** Colour transform {@code i} of the frame: {mult r, g, b, a, add r, g, b, a}. */
	private static float[] ct(int i) {
		float[] out = new float[8];
		System.arraycopy(FRAME, 8 + i * 8, out, 0, 8);
		return out;
	}

	private static void submit(GuiGraphicsExtractor g, Layout layout, Identifier texture, float[] tris, float z, float scale, float[] ct, int channel) {
		if (tris.length == 0) {
			return;
		}
		List<float[]> verts = new ArrayList<>();
		for (int i = 0; i < tris.length; i += 4) {
			verts.add(vertex(layout, tris[i] * scale, tris[i + 1] * scale, z, tris[i + 2], tris[i + 3]));
		}
		g.guiRenderState.addGuiElement(new MeterElement(pipeline, textureSetup(texture), verts, ct, channel));
	}

	private static void submitText(GuiGraphicsExtractor g, Layout layout, Meter m, TextField field, Font font, String text, float z, float[] ct, float[] color, int glowChannel) {
		float k = field.size / font.unitsPerEm;
		float width = 0.0F;
		for (int i = 0; i < text.length(); i++) {
			width += font.advance(text.charAt(i)) * k;
		}
		float inner = field.xmax - field.xmin - 4.0F - field.leftMargin - field.rightMargin;
		float left = field.originX + field.xmin + 2.0F + field.leftMargin + (inner - width) * 0.5F;
		float baseline = field.originY + field.ymin + 2.0F + font.ascent * k;
		// The text field's colour transform turns its colour into a constant, and its (black) glow
		// into the transform's offset: both are drawn as offsets with the alpha from the atlas.
		float[] glyphCt = {0.0F, 0.0F, 0.0F, ct[3], 0, 0, 0, ct[7]};
		float[] glowCt = {0.0F, 0.0F, 0.0F, ct[3], 0, 0, 0, ct[7]};
		for (int c = 0; c < 3; c++) {
			glyphCt[4 + c] = Math.clamp(color[c] * ct[c] + ct[4 + c], 0.0F, 255.0F);
			glowCt[4 + c] = Math.clamp(ct[4 + c], 0.0F, 255.0F);
		}
		List<float[]> glows = new ArrayList<>();
		List<float[]> glyphs = new ArrayList<>();
		float x = left;
		for (int i = 0; i < text.length(); i++) {
			char ch = text.charAt(i);
			Glyph gl = font.glyphs.get(ch);
			if (gl != null) {
				float x0 = x + gl.plane[0] * k;
				float y0 = baseline + gl.plane[1] * k;
				float x1 = x + gl.plane[2] * k;
				float y1 = baseline + gl.plane[3] * k;
				float u0 = gl.uv[0] / font.atlasW;
				float v0 = gl.uv[1] / font.atlasH;
				float u1 = gl.uv[2] / font.atlasW;
				float v1 = gl.uv[3] / font.atlasH;
				for (List<float[]> out : List.of(glows, glyphs)) {
					out.add(vertex(layout, x0, y0, z, u0, v0));
					out.add(vertex(layout, x0, y1, z, u0, v1));
					out.add(vertex(layout, x1, y1, z, u1, v1));
					out.add(vertex(layout, x1, y0, z, u1, v0));
				}
			}
			x += font.advance(ch) * k;
		}
		if (glyphs.isEmpty()) {
			return;
		}
		TextureSetup tex = textureSetup(font.atlas);
		// All glows first, under all glyphs (the filter is applied to the whole field).
		if (glowChannel >= 0) {
			g.guiRenderState.addGuiElement(new MeterElement(pipeline, tex, quadsToTris(glows), glowCt, glowChannel));
		}
		g.guiRenderState.addGuiElement(new MeterElement(pipeline, tex, quadsToTris(glyphs), glyphCt, CHANNEL_GLYPH));
	}

	private static List<float[]> quadsToTris(List<float[]> quads) {
		List<float[]> out = new ArrayList<>();
		for (int i = 0; i < quads.size(); i += 4) {
			out.add(quads.get(i));
			out.add(quads.get(i + 1));
			out.add(quads.get(i + 2));
			out.add(quads.get(i));
			out.add(quads.get(i + 2));
			out.add(quads.get(i + 3));
		}
		return out;
	}

	/** A projected vertex: {gui x, gui y, w, u, v}. */
	private static float[] vertex(Layout layout, float x, float y, float z, float u, float v) {
		float[] p = project(layout, x, y, z);
		return new float[] {p[0] / layout.guiScale, p[1] / layout.guiScale, p[2], u, v};
	}

	private static TextureSetup textureSetup(Identifier id) {
		AbstractTexture t = Minecraft.getInstance().getTextureManager().getTexture(id);
		return TextureSetup.singleTexture(t.getTextureView(), t.getSampler());
	}

	/** Where the HUD puts the meter for a {@code width} x {@code height} pixel window (HUD scale 1). */
	private record Layout(float scale, float x, float y, float guiScale) {
		static Layout of(float width, float height, float guiScale) {
			float fit = width / height >= STAGE_W / STAGE_H ? height / STAGE_H : width / STAGE_W;
			float scale = Math.max(fit, 0.5F);
			return new Layout(scale, STAGE_X + width / scale - STAGE_W, STAGE_Y + height / scale - STAGE_H, guiScale);
		}
	}

	/**
	 * A point of the meter clip (movie px, z towards the screen) in window pixels, plus the
	 * perspective divisor w: {@code BoostMeterLayout::project} of the Rust core.
	 */
	private static float[] project(Layout l, float x, float y, float z) {
		float sx = (float) Math.sin(ROTATION_X);
		float cx = (float) Math.cos(ROTATION_X);
		float sy = (float) Math.sin(ROTATION_Y);
		float cy = (float) Math.cos(ROTATION_Y);
		float y1 = y * cx - z * sx;
		float z1 = y * sx + z * cx;
		float x2 = x * cy + z1 * sy;
		float z2 = -x * sy + z1 * cy;
		float px = l.x + x2;
		float py = l.y + y1;
		float ccx = l.x + CENTER_DX;
		float ccy = l.y + CENTER_DY;
		float w = (FOCAL + z2) / FOCAL;
		return new float[] {(ccx + (px - ccx) / w) * l.scale, (ccy + (py - ccy) / w) * l.scale, w};
	}

	/**
	 * One layer: triangles (as degenerate quads, the GUI draws quads) in the entity vertex format.
	 * Position z carries the perspective divisor; Color the colour transform's multipliers; UV1 /
	 * UV2 its offsets (-255..255), the alpha offset stored as {@code offset + 255 + 512 * channel}
	 * with the alpha channel selector.
	 */
	private record MeterElement(RenderPipeline pipeline, TextureSetup textureSetup, List<float[]> verts, float[] ct, int channel, @Nullable ScreenRectangle bounds)
		implements GuiElementRenderState {
		MeterElement(RenderPipeline pipeline, TextureSetup textureSetup, List<float[]> verts, float[] ct, int channel) {
			this(pipeline, textureSetup, verts, ct, channel, boundsOf(verts));
		}

		private static ScreenRectangle boundsOf(List<float[]> verts) {
			float x0 = Float.MAX_VALUE;
			float y0 = Float.MAX_VALUE;
			float x1 = -Float.MAX_VALUE;
			float y1 = -Float.MAX_VALUE;
			for (float[] v : verts) {
				x0 = Math.min(x0, v[0]);
				y0 = Math.min(y0, v[1]);
				x1 = Math.max(x1, v[0]);
				y1 = Math.max(y1, v[1]);
			}
			int ix = (int) Math.floor(x0);
			int iy = (int) Math.floor(y0);
			return new ScreenRectangle(ix, iy, (int) Math.ceil(x1) - ix, (int) Math.ceil(y1) - iy);
		}

		@Override
		public void buildVertices(VertexConsumer vc) {
			int r = Math.round(Math.clamp(this.ct[0], 0.0F, 1.0F) * 255.0F);
			int g = Math.round(Math.clamp(this.ct[1], 0.0F, 1.0F) * 255.0F);
			int b = Math.round(Math.clamp(this.ct[2], 0.0F, 1.0F) * 255.0F);
			int a = Math.round(Math.clamp(this.ct[3], 0.0F, 1.0F) * 255.0F);
			int ar = Math.round(Math.clamp(this.ct[4], -255.0F, 255.0F));
			int ag = Math.round(Math.clamp(this.ct[5], -255.0F, 255.0F));
			int ab = Math.round(Math.clamp(this.ct[6], -255.0F, 255.0F));
			int aa = Math.round(Math.clamp(this.ct[7], -255.0F, 255.0F)) + 255 + 512 * this.channel;
			for (int i = 0; i + 2 < this.verts.size(); i += 3) {
				for (int k : new int[] {0, 1, 2, 2}) {
					float[] v = this.verts.get(i + k);
					vc.addVertex(v[0], v[1], v[2]).setColor(r, g, b, a).setUv(v[3], v[4]).setUv1(ar, ag).setUv2(ab, aa).setNormal(0.0F, 0.0F, 1.0F);
				}
			}
		}

		@Override
		public @Nullable ScreenRectangle scissorArea() {
			return null;
		}
	}

	// ----------------------------------------------------------------------------- data

	private record TextField(float originX, float originY, float xmin, float xmax, float ymin, float size, float leftMargin, float rightMargin) {
	}

	private record Glyph(float advance, float[] plane, float[] uv) {
	}

	private record Font(Identifier atlas, float atlasW, float atlasH, float unitsPerEm, float ascent, float spaceAdvance, Map<Character, Glyph> glyphs) {
		float advance(char c) {
			Glyph g = this.glyphs.get(c);
			return g != null ? g.advance : this.spaceAdvance;
		}
	}

	private record Meter(
		Identifier background, float[] backgroundTris, Identifier glow, float[] glowTris,
		Identifier fillTexture, List<float[]> fill, Identifier fillTintedTexture, List<float[]> fillTintedFrames,
		TextField backgroundText, TextField label, TextField boostText, Font numbers, Font header
	) {
	}

	/** The extracted meter, or null without one. */
	private static @Nullable Meter meter() {
		if (meterLoaded) {
			return meter;
		}
		meterLoaded = true;
		Path root = RlModels.root();
		Path file = root != null ? root.resolve("hud/boost_meter.json") : null;
		if (file == null || !Files.isRegularFile(file)) {
			return null;
		}
		try {
			JsonObject j = JsonParser.parseString(Files.readString(file)).getAsJsonObject();
			Path dir = file.getParent();
			JsonObject bg = j.getAsJsonObject("background");
			JsonObject glow = j.getAsJsonObject("glow");
			JsonObject fill = j.getAsJsonObject("fill");
			JsonObject tinted = j.getAsJsonObject("fill_tinted");
			JsonObject texts = j.getAsJsonObject("texts");
			JsonObject fonts = j.getAsJsonObject("fonts");
			TextField boostText = textField(texts.getAsJsonObject("boostTextField"));
			TextField label = textField(texts.getAsJsonObject("boostLabel"));
			meter = new Meter(
				RlModels.texture(dir.resolve(bg.get("texture").getAsString())), rectTris(floats(bg.getAsJsonArray("rect"))),
				RlModels.texture(dir.resolve(glow.get("texture").getAsString())), rectTris(floats(glow.getAsJsonArray("rect"))),
				RlModels.texture(dir.resolve(fill.get("texture").getAsString())), frames(fill),
				RlModels.texture(dir.resolve(tinted.get("texture").getAsString())), frames(tinted),
				textField(texts.getAsJsonObject("backgroundTextField")), label, boostText,
				font(dir, fonts.getAsJsonObject(texts.getAsJsonObject("boostTextField").get("font").getAsString())),
				font(dir, fonts.getAsJsonObject(texts.getAsJsonObject("boostLabel").get("font").getAsString()))
			);
			RlCar.LOG.info("RL Car: using Rocket League's boost meter from {}", file.toAbsolutePath());
		} catch (Exception e) {
			RlCar.LOG.error("RL Car: cannot load {}", file, e);
			meter = null;
		}
		return meter;
	}

	private static float[] floats(JsonArray a) {
		float[] out = new float[a.size()];
		for (int i = 0; i < out.length; i++) {
			out[i] = a.get(i).getAsFloat();
		}
		return out;
	}

	/** A bitmap's rectangle {x, y, w, h} as two triangles of {x, y, u, v}. */
	private static float[] rectTris(float[] r) {
		return polygonTris(new float[][] {{r[0], r[1]}, {r[0] + r[2], r[1]}, {r[0] + r[2], r[1] + r[3]}, {r[0], r[1] + r[3]}}, r);
	}

	/** A polygon clipping a bitmap at {@code rect}, fanned around its vertex closest to the origin. */
	private static float[] polygonTris(float[][] poly, float[] rect) {
		int n = poly.length;
		if (n < 3) {
			return new float[0];
		}
		int c = 0;
		for (int i = 1; i < n; i++) {
			if (poly[i][0] * poly[i][0] + poly[i][1] * poly[i][1] < poly[c][0] * poly[c][0] + poly[c][1] * poly[c][1]) {
				c = i;
			}
		}
		float[] out = new float[(n - 2) * 3 * 4];
		int o = 0;
		for (int k = 1; k < n - 1; k++) {
			for (int idx : new int[] {c, (c + k) % n, (c + k + 1) % n}) {
				out[o++] = poly[idx][0];
				out[o++] = poly[idx][1];
				out[o++] = (poly[idx][0] - rect[0]) / rect[2];
				out[o++] = (poly[idx][1] - rect[1]) / rect[3];
			}
		}
		return out;
	}

	private static List<float[]> frames(JsonObject bar) {
		float[] rect = floats(bar.getAsJsonArray("rect"));
		List<float[]> out = new ArrayList<>();
		for (JsonElement f : bar.getAsJsonArray("frames")) {
			JsonArray pts = f.getAsJsonArray();
			float[][] poly = new float[pts.size()][];
			for (int i = 0; i < poly.length; i++) {
				poly[i] = floats(pts.get(i).getAsJsonArray());
			}
			out.add(polygonTris(poly, rect));
		}
		return out;
	}

	private static TextField textField(JsonObject t) {
		float[] origin = floats(t.getAsJsonArray("origin"));
		float[] b = floats(t.getAsJsonArray("bounds"));
		float[] margins = floats(t.getAsJsonArray("margins"));
		return new TextField(origin[0], origin[1], b[0], b[1], b[2], t.get("size").getAsFloat(), margins[0], margins[1]);
	}

	private static Font font(Path dir, JsonObject f) throws java.io.IOException {
		float[] size = floats(f.getAsJsonArray("atlas_size"));
		Map<Character, Glyph> glyphs = new HashMap<>();
		for (Map.Entry<String, JsonElement> e : f.getAsJsonObject("glyphs").entrySet()) {
			JsonObject g = e.getValue().getAsJsonObject();
			glyphs.put(e.getKey().charAt(0), new Glyph(g.get("advance").getAsFloat(), floats(g.getAsJsonArray("plane")), floats(g.getAsJsonArray("uv"))));
		}
		return new Font(RlModels.texture(dir.resolve(f.get("atlas").getAsString())), size[0], size[1], f.get("units_per_em").getAsFloat(),
			f.get("ascent").getAsFloat(), f.get("space_advance").getAsFloat(), glyphs);
	}
}

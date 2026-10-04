package dev.rlcar.client;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.mojang.blaze3d.platform.NativeImage;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.device.GpuDevice;
import com.mojang.renderpearl.api.textures.FilterMode;
import dev.rlcar.RlCar;
import dev.rlcar.physics.RlCarNative;
import java.io.IOException;
import java.io.InputStream;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.texture.AbstractTexture;
import net.minecraft.resources.Identifier;
import org.jspecify.annotations.Nullable;

/**
 * The real Rocket League models (7 car bodies, the wheel and the ball), extracted from the player's own game
 * install by {@code tools/rl_assets/extract.py}. They are read at runtime from a local folder and
 * never shipped with the mod:
 *
 * <ol>
 * <li>the {@code rlcar.assets} system property, else</li>
 * <li>{@code <game dir>/rlcar-assets/} (copy the extracted {@code assets/rl} folder there).</li>
 * </ol>
 *
 * <p>The models use the mod's car model frame directly (+X forward, +Y up, +Z right, blocks =
 * metres, origin at the centre of mass), exactly as in the Bevy demo. The cars and the wheel are
 * drawn with ports of the game's material shaders ({@link RlShading}) when the extraction has
 * their inputs, otherwise with the base colour textures. Without the files, {@link CarRenderer}
 * draws a simple box car instead.
 */
public final class RlModels {
	/** Radius of the exported wheel mesh ({@code WHEEL_Star_SM}), blocks. */
	public static final float WHEEL_MESH_RADIUS = 0.16313F;

	/**
	 * One textured triangle list. Triangles are drawn as quads with a repeated last vertex.
	 * {@code uvs1} is the second UV set (a copy of the first when the mesh has only one).
	 * {@code material} is the glTF material's name (the game's material instance).
	 */
	public record Part(String material, Identifier texture, float[] positions, float[] normals, float[] uvs, float[] uvs1, int[] triangles) {
	}

	public record Model(List<Part> parts) {
	}

	private static final Map<String, @Nullable Model> MODELS = new HashMap<>();
	private static final Map<Integer, float[][]> ANCHORS = new HashMap<>();
	private static final Map<Path, Identifier> TEXTURES = new HashMap<>();
	private static @Nullable Path root;
	private static boolean rootResolved;

	private RlModels() {
	}

	/** The extracted assets folder, or null if there is none. */
	public static @Nullable Path root() {
		if (!rootResolved) {
			rootResolved = true;
			String prop = System.getProperty("rlcar.assets");
			Path dir = prop != null ? Path.of(prop) : FabricLoader.getInstance().getGameDir().resolve("rlcar-assets");
			if (Files.isRegularFile(dir.resolve("wheel/wheel.gltf"))) {
				root = dir;
				RlCar.LOG.info("RL Car: using Rocket League models from {}", dir.toAbsolutePath());
			} else {
				RlCar.LOG.info("RL Car: no extracted Rocket League models in {} (see minecraft/README.md); drawing box cars", dir.toAbsolutePath());
			}
		}
		return root;
	}

	public static @Nullable Model body(int preset, boolean orange) {
		return model("cars/" + RlCarNative.PRESETS[preset] + "/body_" + (orange ? "orange" : "blue") + ".gltf");
	}

	public static @Nullable Model wheel() {
		return model("wheel/wheel.gltf");
	}

	/** The ball, centred on its origin (blocks), or null without it. */
	public static @Nullable Model ball() {
		return model("ball/ball.gltf");
	}

	/** A model of the extracted boost ({@code boost/<file>}), e.g. a car's flame cones. */
	public static @Nullable Model boostModel(String file) {
		return model("boost/" + file);
	}

	/** The model's wheel hubs (FL, FR, BL, BR), {x, y, z} in model space, or null. */
	public static float @Nullable [][] anchors(int preset) {
		return ANCHORS.computeIfAbsent(preset, p -> {
			Path dir = root();
			if (dir == null) {
				return null;
			}
			try {
				float[][] out = new float[4][];
				List<String> lines = Files.readAllLines(dir.resolve("cars/" + RlCarNative.PRESETS[p] + "/wheels.txt"));
				String[] corners = {"FL", "FR", "BL", "BR"};
				for (int i = 0; i < 4; i++) {
					for (String line : lines) {
						String[] f = line.trim().split("\\s+");
						if (f.length == 4 && f[0].equals(corners[i])) {
							out[i] = new float[] {Float.parseFloat(f[1]), Float.parseFloat(f[2]), Float.parseFloat(f[3])};
						}
					}
					if (out[i] == null) {
						return null;
					}
				}
				return out;
			} catch (IOException | NumberFormatException e) {
				return null;
			}
		});
	}

	private static @Nullable Model model(String file) {
		if (MODELS.containsKey(file)) {
			return MODELS.get(file);
		}
		Model m = null;
		Path dir = root();
		if (dir != null && Files.isRegularFile(dir.resolve(file))) {
			try {
				m = loadGltf(dir.resolve(file));
			} catch (Exception e) {
				RlCar.LOG.error("RL Car: cannot load {}", file, e);
			}
		}
		MODELS.put(file, m);
		return m;
	}

	// ----------------------------------------------------------------------------- glTF

	private static Model loadGltf(Path file) throws IOException {
		JsonObject g = JsonParser.parseString(Files.readString(file)).getAsJsonObject();
		Path dir = file.getParent();
		List<ByteBuffer> buffers = new ArrayList<>();
		for (JsonElement b : g.getAsJsonArray("buffers")) {
			buffers.add(ByteBuffer.wrap(Files.readAllBytes(dir.resolve(b.getAsJsonObject().get("uri").getAsString()))).order(ByteOrder.LITTLE_ENDIAN));
		}
		List<Part> parts = new ArrayList<>();
		for (JsonElement mesh : g.getAsJsonArray("meshes")) {
			for (JsonElement pe : mesh.getAsJsonObject().getAsJsonArray("primitives")) {
				JsonObject prim = pe.getAsJsonObject();
				JsonObject attr = prim.getAsJsonObject("attributes");
				if (prim.has("mode") && prim.get("mode").getAsInt() != 4) {
					continue; // triangles only
				}
				float[] pos = floats(g, buffers, attr.get("POSITION").getAsInt(), 3);
				float[] nrm = attr.has("NORMAL") ? floats(g, buffers, attr.get("NORMAL").getAsInt(), 3) : new float[pos.length];
				float[] uv = attr.has("TEXCOORD_0") ? floats(g, buffers, attr.get("TEXCOORD_0").getAsInt(), 2) : new float[pos.length / 3 * 2];
				float[] uv1 = attr.has("TEXCOORD_1") ? floats(g, buffers, attr.get("TEXCOORD_1").getAsInt(), 2) : uv;
				int[] tris = prim.has("indices") ? ints(g, buffers, prim.get("indices").getAsInt()) : sequence(pos.length / 3);
				int material = prim.has("material") ? prim.get("material").getAsInt() : -1;
				Identifier tex = baseColor(g, dir, material);
				JsonObject mat = material >= 0 ? g.getAsJsonArray("materials").get(material).getAsJsonObject() : null;
				String name = mat != null && mat.has("name") ? mat.get("name").getAsString() : "";
				parts.add(new Part(name, tex, pos, nrm, uv, uv1, tris));
			}
		}
		return new Model(parts);
	}

	private static int[] sequence(int n) {
		int[] out = new int[n];
		for (int i = 0; i < n; i++) {
			out[i] = i;
		}
		return out;
	}

	/** Start (absolute byte offset), stride and buffer of an accessor. */
	private static ByteBuffer view(JsonObject g, List<ByteBuffer> buffers, JsonObject acc, int[] startStride, int elementSize) {
		JsonObject bv = g.getAsJsonArray("bufferViews").get(acc.get("bufferView").getAsInt()).getAsJsonObject();
		startStride[0] = (bv.has("byteOffset") ? bv.get("byteOffset").getAsInt() : 0) + (acc.has("byteOffset") ? acc.get("byteOffset").getAsInt() : 0);
		startStride[1] = bv.has("byteStride") ? bv.get("byteStride").getAsInt() : elementSize;
		return buffers.get(bv.get("buffer").getAsInt());
	}

	private static float[] floats(JsonObject g, List<ByteBuffer> buffers, int accessor, int components) {
		JsonObject acc = g.getAsJsonArray("accessors").get(accessor).getAsJsonObject();
		if (acc.get("componentType").getAsInt() != 5126) {
			throw new IllegalArgumentException("accessor " + accessor + " is not float");
		}
		int count = acc.get("count").getAsInt();
		int[] ss = new int[2];
		ByteBuffer buf = view(g, buffers, acc, ss, components * 4);
		float[] out = new float[count * components];
		for (int i = 0; i < count; i++) {
			for (int c = 0; c < components; c++) {
				out[i * components + c] = buf.getFloat(ss[0] + i * ss[1] + c * 4);
			}
		}
		return out;
	}

	private static int[] ints(JsonObject g, List<ByteBuffer> buffers, int accessor) {
		JsonObject acc = g.getAsJsonArray("accessors").get(accessor).getAsJsonObject();
		int type = acc.get("componentType").getAsInt();
		int size = type == 5125 ? 4 : type == 5123 ? 2 : 1;
		int count = acc.get("count").getAsInt();
		int[] ss = new int[2];
		ByteBuffer buf = view(g, buffers, acc, ss, size);
		int[] out = new int[count];
		for (int i = 0; i < count; i++) {
			int at = ss[0] + i * ss[1];
			out[i] = size == 4 ? buf.getInt(at) : size == 2 ? Short.toUnsignedInt(buf.getShort(at)) : Byte.toUnsignedInt(buf.get(at));
		}
		return out;
	}

	private static Identifier baseColor(JsonObject g, Path dir, int material) throws IOException {
		if (material >= 0) {
			JsonObject mat = g.getAsJsonArray("materials").get(material).getAsJsonObject();
			JsonObject pbr = mat.getAsJsonObject("pbrMetallicRoughness");
			if (pbr != null && pbr.has("baseColorTexture")) {
				int tex = pbr.getAsJsonObject("baseColorTexture").get("index").getAsInt();
				int image = g.getAsJsonArray("textures").get(tex).getAsJsonObject().get("source").getAsInt();
				JsonArray images = g.getAsJsonArray("images");
				return texture(dir.resolve(images.get(image).getAsJsonObject().get("uri").getAsString()));
			}
		}
		return Identifier.withDefaultNamespace("textures/block/white_concrete.png");
	}

	// ------------------------------------------------------------------------- textures

	/** Registers (once) a PNG as a mipmapped, linearly filtered, repeating texture. */
	static Identifier texture(Path png) throws IOException {
		Path key = png.toAbsolutePath().normalize();
		Identifier id = TEXTURES.get(key);
		if (id != null) {
			return id;
		}
		String rel = (root() != null ? root().toAbsolutePath().normalize().relativize(key) : key.getFileName()).toString();
		id = RlCar.id("rl_models/" + rel.replace('\\', '/').toLowerCase(Locale.ROOT).replaceAll("[^a-z0-9/._-]", "_"));
		NativeImage image;
		try (InputStream in = Files.newInputStream(png)) {
			image = NativeImage.read(in);
		}
		Minecraft.getInstance().getTextureManager().register(id, new MipmappedTexture(id.toString(), image));
		TEXTURES.put(key, id);
		return id;
	}

	/**
	 * A texture with a full mip chain and linear filtering. Minecraft's own dynamic textures are
	 * nearest-filtered without mips, which makes 2K car textures shimmer at a distance.
	 */
	private static final class MipmappedTexture extends AbstractTexture {
		MipmappedTexture(String label, NativeImage base) {
			List<NativeImage> levels = new ArrayList<>();
			levels.add(base);
			// Down to 1 texel on the short side (a longer chain does not fit non-square textures).
			while (levels.getLast().getWidth() > 1 && levels.getLast().getHeight() > 1) {
				levels.add(halve(levels.getLast()));
			}
			GpuDevice device = RenderSystem.getDevice();
			this.texture = device.createTexture(label, 5, GpuFormat.RGBA8_UNORM, base.getWidth(), base.getHeight(), 1, levels.size());
			this.textureView = device.createTextureView(this.texture);
			this.sampler = RenderSystem.getSamplerCache().getRepeat(FilterMode.LINEAR, true);
			var encoder = device.createCommandEncoder();
			for (int i = 0; i < levels.size(); i++) {
				encoder.writeToTexture(this.texture, levels.get(i), i, 0, 0, 0);
				levels.get(i).close();
			}
		}

		/** Box-filtered half-size copy (each 8-bit channel averaged on its own). */
		private static NativeImage halve(NativeImage src) {
			int w = Math.max(1, src.getWidth() / 2);
			int h = Math.max(1, src.getHeight() / 2);
			NativeImage out = new NativeImage(w, h, false);
			for (int y = 0; y < h; y++) {
				for (int x = 0; x < w; x++) {
					int x0 = Math.min(x * 2, src.getWidth() - 1), x1 = Math.min(x * 2 + 1, src.getWidth() - 1);
					int y0 = Math.min(y * 2, src.getHeight() - 1), y1 = Math.min(y * 2 + 1, src.getHeight() - 1);
					int a = src.getPixel(x0, y0), b = src.getPixel(x1, y0), c = src.getPixel(x0, y1), d = src.getPixel(x1, y1);
					int px = 0;
					for (int shift = 0; shift < 32; shift += 8) {
						int sum = (a >>> shift & 0xFF) + (b >>> shift & 0xFF) + (c >>> shift & 0xFF) + (d >>> shift & 0xFF);
						px |= ((sum + 2) / 4) << shift;
					}
					out.setPixel(x, y, px);
				}
			}
			return out;
		}
	}
}

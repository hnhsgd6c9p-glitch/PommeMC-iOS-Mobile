import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Set;

import net.fabricmc.loader.api.metadata.ModEnvironment;
import net.fabricmc.loader.impl.FabricLoaderImpl;
import net.fabricmc.loader.impl.game.GameProvider;
import net.fabricmc.loader.impl.game.patch.GameTransformer;
import net.fabricmc.loader.impl.metadata.BuiltinModMetadata;
import net.fabricmc.loader.impl.launch.FabricLauncher;
import net.fabricmc.loader.impl.util.Arguments;

/**
 * Fabric Loader game provider for Pomme's Rust-rendered Minecraft client.
 *
 * This provider deliberately does not pretend that a Java Minecraft client
 * exists. Fabric Loader owns mod discovery, dependency resolution, entrypoint
 * lifecycle and Mixin setup; Pomme supplies the game-facing bridge classes.
 */
public final class PommeGameProvider implements GameProvider {
    private static Class<?> activeEntrypoint;
    private final Arguments arguments = new Arguments();
    private final GameTransformer transformer = new GameTransformer();
    private FabricLauncher fabricLauncher;

    private Path gameDir() {
        return Path.of(System.getProperty("pomme.gameDir", System.getProperty("user.dir", ".")))
            .toAbsolutePath().normalize();
    }

    @Override public String getGameId() { return "pomme"; }
    @Override public String getGameName() { return "Pomme"; }
    @Override public String getRawGameVersion() {
        return System.getProperty("pomme.minecraftVersion", "unknown");
    }
    @Override public String getNormalizedGameVersion() { return getRawGameVersion(); }
    @Override
    public Collection<BuiltinMod> getBuiltinMods() {
        var minecraftMetadata = new BuiltinModMetadata.Builder("minecraft", getRawGameVersion())
            .setEnvironment(ModEnvironment.CLIENT)
            .setName("Minecraft")
            .setDescription("Pomme's Rust-rendered Minecraft client API provider")
            .build();
        List<BuiltinMod> mods = new ArrayList<>();
        mods.add(new BuiltinMod(Collections.emptyList(), minecraftMetadata));

        String intermediaryPath = System.getProperty("pomme.intermediaryJar");
        if (intermediaryPath != null && !intermediaryPath.isBlank()) {
            Path intermediaryJar = Path.of(intermediaryPath);
            if (Files.isRegularFile(intermediaryJar)) {
                var intermediaryMetadata = new BuiltinModMetadata.Builder(
                    "intermediary", getRawGameVersion()
                ).setName("Intermediary mappings").build();
                mods.add(new BuiltinMod(Collections.singletonList(intermediaryJar), intermediaryMetadata));
            }
        }
        return mods;
    }
    @Override public String getEntrypoint() { return "PommeGameEntrypoint"; }
    @Override public Path getLaunchDirectory() { return gameDir(); }
    @Override public boolean requiresUrlClassLoader() { return true; }
    @Override public Set<BuiltinTransform> getBuiltinTransforms(String namespace) {
        return Collections.emptySet();
    }
    @Override public boolean isEnabled() { return true; }

    @Override
    public boolean locateGame(FabricLauncher launcher, String[] args) {
        arguments.parse(args);
        return true;
    }

    @Override
    public void initialize(FabricLauncher launcher) {
        this.fabricLauncher = launcher;
        String modsDir = System.getProperty("pomme.modsDir");
        if (modsDir != null && !modsDir.isBlank()) {
            System.setProperty("fabric.modsFolder", Path.of(modsDir).toAbsolutePath().normalize().toString());
        }
        System.setProperty("fabric.side", "client");
        System.setProperty("fabric.development", "false");
        transformer.locateEntrypoints(launcher, launcher.getClassPath());
    }

    @Override public GameTransformer getEntrypointTransformer() { return transformer; }
    @Override public void unlockClassPath(FabricLauncher launcher) { }
    @Override public Arguments getArguments() { return arguments; }
    @Override public String[] getLaunchArguments(boolean sanitize) { return arguments.toArray(); }
    @Override public String getRuntimeNamespace(String defaultNamespace) { return "intermediary"; }
    @Override public String getDefaultModDistributionNamespace(String defaultNamespace) { return "intermediary"; }

    @Override
    public void launch(ClassLoader loader) {
        try {
            Class<?> entrypoint = fabricLauncher.loadIntoTarget(getEntrypoint());
            activeEntrypoint = entrypoint;
            entrypoint.getMethod("main", String[].class).invoke(null, (Object) new String[0]);
        } catch (ReflectiveOperationException error) {
            throw new RuntimeException("Could not start Pomme's Fabric game bridge", error);
        }
    }

    private static void dispatch(String methodName) {
        if (activeEntrypoint == null) {
            throw new IllegalStateException("Fabric game entrypoint is not active");
        }
        try {
            activeEntrypoint.getMethod(methodName).invoke(null);
        } catch (ReflectiveOperationException error) {
            throw new RuntimeException("Could not dispatch Fabric callback " + methodName, error);
        }
    }

    public static void dispatchClientTickStart() { dispatch("onClientTickStart"); }
    public static void dispatchClientTickEnd() { dispatch("onClientTickEnd"); }
    public static void dispatchHudRender() { dispatch("onHudRender"); }

    /** Integration-test probe, also useful for runtime diagnostics. */
    public static int getProbeTicks() {
        return Integer.parseInt(System.getProperty("pomme.probe.ticks", "0"));
    }

    public static int getProbeX() {
        return (int) Double.parseDouble(System.getProperty("pomme.probe.x", "0"));
    }

    public static int getProbeHealth() {
        return (int) Float.parseFloat(System.getProperty("pomme.probe.health", "0"));
    }

    public static int getProbeWorldTicks() {
        return Integer.parseInt(System.getProperty("pomme.probe.worldTicks", "0"));
    }

    public static int getProbeEndWorldTicks() {
        return Integer.parseInt(System.getProperty("pomme.probe.endWorldTicks", "0"));
    }

    public static int getProbeEndClientTicks() {
        return Integer.parseInt(System.getProperty("pomme.probe.endClientTicks", "0"));
    }

    /** Compatibility-test probes for a real third-party Fabric mod. */
    public static int isAppleSkinLoaded() {
        return FabricLoaderImpl.INSTANCE.isModLoaded("appleskin") ? 1 : 0;
    }

    public static int isOfficialFabricApiLoaded() {
        return FabricLoaderImpl.INSTANCE.isModLoaded("fabric-api") ? 1 : 0;
    }

    public static int isAppleSkinClassAvailable() {
        if (activeEntrypoint == null) return 0;
        try {
            activeEntrypoint.getClassLoader().loadClass("squeek.appleskin.AppleSkin");
            return 1;
        } catch (ClassNotFoundException error) {
            return 0;
        }
    }

    /** Compatibility probes for the real third-party Lithium Fabric mod. */
    public static int isLithiumLoaded() {
        return FabricLoaderImpl.INSTANCE.isModLoaded("lithium") ? 1 : 0;
    }

    public static int isLithiumInitialized() {
        if (activeEntrypoint == null) return 0;
        try {
            Class<?> lithium = activeEntrypoint.getClassLoader()
                .loadClass("net.caffeinemc.mods.lithium.common.LithiumMod");
            Object version = lithium.getMethod("getVersion").invoke(null);
            return version == null ? 0 : 1;
        } catch (ReflectiveOperationException | RuntimeException error) {
            return 0;
        }
    }

    public static int isLithiumMixinClassAvailable() {
        if (activeEntrypoint == null) return 0;
        try {
            activeEntrypoint.getClassLoader().loadClass(
                "net.caffeinemc.mods.lithium.mixin.block.hopper.HopperBlockEntityMixin"
            );
            return 1;
        } catch (ClassNotFoundException error) {
            return 0;
        }
    }

    public static int isLithiumHopperMixinApplied() {
        if (activeEntrypoint == null) return 0;
        try {
            Class<?> hopper = activeEntrypoint.getClassLoader().loadClass("net.minecraft.class_2614");
            hopper.getDeclaredMethod("lithium$invalidateCacheOnNeighborUpdate", boolean.class);
            return 1;
        } catch (ReflectiveOperationException error) {
            return 0;
        }
    }

    /** Inspect the transformed Minecraft compatibility class through Knot's loader. */
    public static int isProbeMixinApplied() {
        if (activeEntrypoint == null) return 0;
        try {
            ClassLoader loader = activeEntrypoint.getClassLoader();
            Class<?> clientClass = loader.loadClass("net.minecraft.class_310");
            Object client = clientClass.getMethod("getInstance").invoke(null);
            return Boolean.TRUE.equals(clientClass.getMethod("isPaused").invoke(client)) ? 1 : 0;
        } catch (ReflectiveOperationException error) {
            throw new RuntimeException("Could not inspect the transformed Minecraft client bridge", error);
        }
    }
}

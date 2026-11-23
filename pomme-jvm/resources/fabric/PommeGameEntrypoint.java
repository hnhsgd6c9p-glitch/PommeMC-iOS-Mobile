import java.nio.file.Path;

import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.api.ModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.loader.api.entrypoint.PreLaunchEntrypoint;
import net.fabricmc.loader.impl.FabricLoaderImpl;
import net.minecraft.class_310;
import net.minecraft.class_638;

/**
 * Executes Fabric's actual lifecycle after FabricLoaderImpl has completed
 * discovery and dependency resolution.
 */
public final class PommeGameEntrypoint {
    private static boolean missingWorldLogged;

    private PommeGameEntrypoint() { }

    /** A live world is bound only when the native game loop publishes one. */
    private static boolean hasWorld() {
        return Boolean.getBoolean("pomme.worldBound");
    }

    /** One-shot debug note; world ticks are meaningless while only the dummy world exists. */
    private static void logMissingWorldOnce() {
        if (missingWorldLogged || !Boolean.getBoolean("pomme.debug")) {
            return;
        }
        missingWorldLogged = true;
        System.err.println("[pomme] world tick suppressed: no live world bound");
    }

    public static void main(String[] args) {
        Path gameDir = Path.of(System.getProperty("pomme.gameDir", ".")).toAbsolutePath().normalize();
        class_310 context = class_310.getInstance();
        context.initialize(gameDir, System.getProperty("pomme.minecraftVersion", "unknown"));
        FabricLoaderImpl loader = FabricLoaderImpl.INSTANCE;
        loader.prepareModInit(gameDir, context);
        loader.invokeEntrypoints("preLaunch", PreLaunchEntrypoint.class, PreLaunchEntrypoint::onPreLaunch);
        loader.invokeEntrypoints("main", ModInitializer.class, ModInitializer::onInitialize);
        loader.invokeEntrypoints("client", ClientModInitializer.class, ClientModInitializer::onInitializeClient);
    }

    /** Native game-loop dispatch points; Fabric API events are attached here. */
    public static void onClientTickStart() {
        class_310 client = class_310.getInstance();
        client.runTasks();
        ClientTickEvents.START_CLIENT_TICK.invoker().onStartTick(client);
        if (hasWorld()) {
            ClientTickEvents.START_WORLD_TICK.invoker().onStartTick(class_638.INSTANCE);
        } else {
            logMissingWorldOnce();
        }
    }
    public static void onClientTickEnd() {
        if (hasWorld()) {
            ClientTickEvents.END_WORLD_TICK.invoker().onEndTick(class_638.INSTANCE);
        } else {
            logMissingWorldOnce();
        }
        ClientTickEvents.END_CLIENT_TICK.invoker().onEndTick(class_310.getInstance());
    }
    /** Not implemented: no Fabric HUD draw callback is bridged yet. */
    public static void onHudRender() { }

}

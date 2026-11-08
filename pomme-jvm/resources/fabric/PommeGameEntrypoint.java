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
    private PommeGameEntrypoint() { }

    public static void main(String[] args) {
        System.err.println("[POMME_TRACE] PommeGameEntrypoint.main: begin");
        Path gameDir = Path.of(System.getProperty("pomme.gameDir", ".")).toAbsolutePath().normalize();
        System.err.println("[POMME_TRACE] Before class_310.getInstance");
        class_310 context = class_310.getInstance();
        System.err.println("[POMME_TRACE] After class_310.getInstance");
        context.initialize(gameDir, System.getProperty("pomme.minecraftVersion", "unknown"));
        System.err.println("[POMME_TRACE] After Minecraft bridge initialize");
        FabricLoaderImpl loader = FabricLoaderImpl.INSTANCE;
        System.err.println("[POMME_TRACE] Before FabricLoaderImpl.prepareModInit");
        loader.prepareModInit(gameDir, context);
        System.err.println("[POMME_TRACE] After FabricLoaderImpl.prepareModInit");
        System.err.println("[POMME_TRACE] Before preLaunch entrypoints");
        loader.invokeEntrypoints("preLaunch", PreLaunchEntrypoint.class, PreLaunchEntrypoint::onPreLaunch);
        System.err.println("[POMME_TRACE] After preLaunch entrypoints");
        System.err.println("[POMME_TRACE] Before main entrypoints");
        loader.invokeEntrypoints("main", ModInitializer.class, ModInitializer::onInitialize);
        System.err.println("[POMME_TRACE] After main entrypoints");
        System.err.println("[POMME_TRACE] Before client entrypoints");
        loader.invokeEntrypoints("client", ClientModInitializer.class, ClientModInitializer::onInitializeClient);
        System.err.println("[POMME_TRACE] After client entrypoints");
    }

    /** Native game-loop dispatch points; Fabric API events are attached here. */
    public static void onClientTickStart() {
        class_310 client = class_310.getInstance();
        client.runTasks();
        ClientTickEvents.START_CLIENT_TICK.invoker().onStartTick(client);
        ClientTickEvents.START_WORLD_TICK.invoker().onStartTick(class_638.INSTANCE);
    }
    public static void onClientTickEnd() {
        ClientTickEvents.END_WORLD_TICK.invoker().onEndTick(class_638.INSTANCE);
        ClientTickEvents.END_CLIENT_TICK.invoker().onEndTick(class_310.getInstance());
    }
    public static void onHudRender() { }

}

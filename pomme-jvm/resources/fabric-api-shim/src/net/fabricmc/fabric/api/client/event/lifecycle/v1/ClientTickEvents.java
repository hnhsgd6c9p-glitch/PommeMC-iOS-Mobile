package net.fabricmc.fabric.api.client.event.lifecycle.v1;

import net.fabricmc.fabric.api.event.Event;
import net.fabricmc.fabric.api.event.EventFactory;
import net.minecraft.class_310;
import net.minecraft.class_638;

/** Client and world tick callbacks dispatched by Pomme's native fixed-step loop. */
public final class ClientTickEvents {
    private ClientTickEvents() { }

    public static final Event<StartTick> START_CLIENT_TICK = EventFactory.createArrayBacked(
        StartTick.class,
        callbacks -> client -> {
            for (StartTick callback : callbacks) callback.onStartTick(client);
        }
    );

    public static final Event<EndTick> END_CLIENT_TICK = EventFactory.createArrayBacked(
        EndTick.class,
        callbacks -> client -> {
            for (EndTick callback : callbacks) callback.onEndTick(client);
        }
    );

    public static final Event<StartWorldTick> START_WORLD_TICK = EventFactory.createArrayBacked(
        StartWorldTick.class,
        callbacks -> world -> {
            for (StartWorldTick callback : callbacks) callback.onStartTick(world);
        }
    );

    public static final Event<EndWorldTick> END_WORLD_TICK = EventFactory.createArrayBacked(
        EndWorldTick.class,
        callbacks -> world -> {
            for (EndWorldTick callback : callbacks) callback.onEndTick(world);
        }
    );

    @FunctionalInterface public interface StartTick { void onStartTick(class_310 client); }
    @FunctionalInterface public interface EndTick { void onEndTick(class_310 client); }
    @FunctionalInterface public interface StartWorldTick { void onStartTick(class_638 world); }
    @FunctionalInterface public interface EndWorldTick { void onEndTick(class_638 world); }
}

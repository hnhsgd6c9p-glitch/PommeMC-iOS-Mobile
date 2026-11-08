package net.fabricmc.fabric.api.event;

import net.minecraft.class_2960;

/** Core Fabric Event ABI, backed by Pomme's thread-safe callback registry. */
public abstract class Event<T> {
    public static final class_2960 DEFAULT_PHASE = class_2960.method_60655("fabric", "default");
    protected volatile T invoker;

    public Event() { }

    public final T invoker() {
        return invoker;
    }

    public abstract void register(T listener);

    public void register(class_2960 phase, T listener) {
        register(listener);
    }

    public void addPhaseOrdering(class_2960 first, class_2960 second) { }
}

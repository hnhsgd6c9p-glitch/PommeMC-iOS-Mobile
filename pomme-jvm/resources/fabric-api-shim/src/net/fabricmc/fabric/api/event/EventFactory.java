package net.fabricmc.fabric.api.event;

import java.lang.reflect.Array;
import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.function.Function;

import net.minecraft.class_2960;

/** Fabric EventFactory methods used by client and common API modules. */
public final class EventFactory {
    private EventFactory() { }

    @SuppressWarnings("unchecked")
    public static <T> Event<T> createArrayBacked(
        Class<? super T> type,
        Function<T[], T> invokerFactory
    ) {
        return createArrayBacked((Class<T>) type, null, invokerFactory);
    }

    public static <T> Event<T> createArrayBacked(
        Class<T> type,
        T defaultValue,
        Function<T[], T> invokerFactory
    ) {
        return new Event<T>() {
            private final List<T> listeners = new CopyOnWriteArrayList<>();

            {
                invoker = defaultValue != null ? defaultValue : createInvoker();
            }

            @SuppressWarnings("unchecked")
            private T createInvoker() {
                T[] callbacks = (T[]) Array.newInstance(type, listeners.size());
                return invokerFactory.apply(listeners.toArray(callbacks));
            }

            @Override
            public void register(T listener) {
                if (listener == null) throw new NullPointerException("listener");
                listeners.add(listener);
                invoker = createInvoker();
            }
        };
    }

    public static <T> Event<T> createWithPhases(
        Class<? super T> type,
        Function<T[], T> invokerFactory,
        class_2960... phases
    ) {
        return createArrayBacked(type, invokerFactory);
    }

    public static String getHandlerName(Object handler) {
        return handler == null ? "null" : handler.getClass().getName();
    }

    public static boolean isProfilingEnabled() { return false; }
    public static void invalidate() { }
}

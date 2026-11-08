package pomme.bridge;

import java.util.Objects;
import java.util.concurrent.ConcurrentLinkedQueue;

/** Shared, allocation-light state channel from Pomme's Rust game loop to Java API shims. */
public final class PommeGameBridge {
    private static final String DEFAULT_STATE = "0,0,0,20,20,20,0,0,0,0,0,true,false,false,false,false,false,0,false";
    private static final ConcurrentLinkedQueue<Runnable> MAIN_THREAD_TASKS = new ConcurrentLinkedQueue<>();
    private static volatile Thread mainThread;
    private static volatile String cachedRaw;
    private static volatile String[] cachedState;

    private PommeGameBridge() { }

    /** Bind the Java game thread when Pomme creates its Minecraft compatibility client. */
    public static void bindMainThread() {
        mainThread = Thread.currentThread();
    }

    /** Match MinecraftClient.execute: run inline on the game thread, otherwise enqueue. */
    public static void execute(Runnable task) {
        Objects.requireNonNull(task, "task");
        Thread owner = mainThread;
        if (owner == null || owner == Thread.currentThread()) {
            task.run();
        } else {
            MAIN_THREAD_TASKS.add(task);
        }
    }

    /** Drain work posted by networking and other worker threads at the next client tick. */
    public static int drainMainThreadTasks() {
        Thread owner = mainThread;
        if (owner != null && owner != Thread.currentThread()) {
            throw new IllegalStateException("Pomme game tasks must be drained on the bound game thread");
        }
        int count = 0;
        Runnable task;
        while ((task = MAIN_THREAD_TASKS.poll()) != null) {
            task.run();
            count++;
        }
        return count;
    }

    private static String[] state() {
        String raw = System.getProperty("pomme.state", DEFAULT_STATE);
        if (!raw.equals(cachedRaw)) {
            synchronized (PommeGameBridge.class) {
                if (!raw.equals(cachedRaw)) {
                    cachedState = raw.split(",", -1);
                    cachedRaw = raw;
                }
            }
        }
        return cachedState;
    }

    public static double getDouble(int index) {
        try { return Double.parseDouble(state()[index]); }
        catch (RuntimeException ignored) { return 0.0; }
    }

    public static float getFloat(int index) {
        return (float) getDouble(index);
    }

    public static int getInt(int index) {
        try { return Integer.parseInt(state()[index]); }
        catch (RuntimeException ignored) { return 0; }
    }

    public static boolean getBoolean(int index) {
        try { return Boolean.parseBoolean(state()[index]); }
        catch (RuntimeException ignored) { return false; }
    }
}

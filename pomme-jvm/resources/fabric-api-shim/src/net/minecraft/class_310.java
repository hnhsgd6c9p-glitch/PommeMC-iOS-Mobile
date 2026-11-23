package net.minecraft;

import java.nio.file.Path;

import pomme.bridge.PommeGameBridge;

/** Initial intermediary MinecraftClient surface exposed by Pomme. */
public final class class_310 {
    private static final class_310 INSTANCE = new class_310();
    public class_746 field_1724 = class_746.INSTANCE;
    public class_638 field_1687 = class_638.INSTANCE;
    public class_315 field_1690 = new class_315();
    public class_636 field_1761 = new class_636();
    private Path runDirectory = Path.of(".").toAbsolutePath().normalize();
    private String launchedVersion = System.getProperty("pomme.minecraftVersion", "unknown");

    private class_310() { }

    public void initialize(Path gameDir, String version) {
        runDirectory = gameDir.toAbsolutePath().normalize();
        launchedVersion = version;
        PommeGameBridge.bindMainThread();
    }

    /** Execute work on the client thread, queuing calls made by worker threads. */
    public void execute(Runnable task) { PommeGameBridge.execute(task); }

    /** Called once per Pomme tick to run work posted by background threads. */
    public void runTasks() { PommeGameBridge.drainMainThreadTasks(); }

    public static class_310 method_1551() { return INSTANCE; }
    public static class_310 getInstance() { return INSTANCE; }
    public String getLaunchedVersion() { return launchedVersion; }
    public Path getRunDirectory() { return runDirectory; }
    public Path method_1490() { return runDirectory; }
    public class_746 getPlayer() { return field_1724; }
    public class_638 getWorld() { return field_1687; }
    public boolean method_1542() { return PommeGameBridge.getBoolean(18); }
    public boolean isPaused() { return PommeGameBridge.getBoolean(15); }
}

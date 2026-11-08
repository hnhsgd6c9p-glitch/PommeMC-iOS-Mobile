import net.fabricmc.loader.impl.launch.knot.KnotClient;

/** Sets Pomme's game paths before starting the official Fabric Knot launcher. */
public final class PommeFabricLauncher {
    private PommeFabricLauncher() { }

    public static void main(String[] args) {
        // JMX is a desktop monitoring facility; HotSpot management natives are absent on mobile.
        System.setProperty("log4j2.disable.jmx", "true");
        if (args.length != 4) {
            throw new IllegalArgumentException("Expected gameDir, modsDir, Minecraft version and intermediary mappings JAR");
        }
        System.setProperty("pomme.gameDir", args[0]);
        System.setProperty("pomme.modsDir", args[1]);
        System.setProperty("pomme.minecraftVersion", args[2]);
        System.setProperty("pomme.intermediaryJar", args[3]);
        System.setProperty("fabric.modsFolder", args[1]);
        System.setProperty("fabric.side", "client");
        System.err.println("[POMME_TRACE] Before KnotClient.main");
        KnotClient.main(new String[0]);
        System.err.println("[POMME_TRACE] After KnotClient.main");
    }
}

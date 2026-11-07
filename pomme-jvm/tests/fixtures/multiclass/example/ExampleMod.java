package example;
import net.fabricmc.api.ModInitializer;
import net.fabricmc.loader.api.FabricLoader;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import pomme.api.Pomme;
public final class ExampleMod implements ModInitializer {
    public static int startupValue = 41;
    public static String startupMessage = "static string field works";
    private static final Logger LOGGER = LoggerFactory.getLogger(ExampleMod.class);
    private int instanceCount;
    public ExampleMod() { this.instanceCount = 7; }
    private void logInstanceValue() { Pomme.logInt(instanceCount); }
    @Override public void onInitialize() {
        LOGGER.info("SLF4J bridge works");
        if (FabricLoader.getInstance().getEnvironmentType().isClient()) {
            LOGGER.info("client environment bridge works");
        }
        if (FabricLoader.getInstance().isModLoaded("helper_lib")) {
            LOGGER.info("FabricLoader dependency lookup works");
        }
        logInstanceValue();
        Helper.logStartupValue();
        Helper.logStartupMessage();
        Helper.logArrayValue();
        Helper.logObjectValue();
        startupValue = Helper.add(2, 3);
        Helper.logStartupValue();
        Helper.logObjectValue();
        Helper.emitStartupMessage("Cross-class Java execution works");
    }
}

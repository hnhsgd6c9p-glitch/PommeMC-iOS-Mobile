package example;

import net.fabricmc.api.ModInitializer;
import pomme.api.Pomme;

public final class ExampleMod implements ModInitializer {
    public ExampleMod() {}

    @Override
    public void onInitialize() {
        Pomme.log("Hello from a Java Fabric mod!");
    }
}

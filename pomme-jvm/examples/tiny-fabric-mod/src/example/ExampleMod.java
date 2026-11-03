package example;

import pomme.api.Pomme;

public final class ExampleMod {
    public ExampleMod() {}

    public void onInitialize() {
        Pomme.log("Hello from a Java Fabric-style mod!");
    }
}

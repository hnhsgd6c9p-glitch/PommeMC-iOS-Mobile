package net.fabricmc.loader.api;
public final class FabricLoader {
    public static FabricLoader getInstance() { return null; }
    public boolean isModLoaded(String id) { return false; }
    public net.fabricmc.api.EnvType getEnvironmentType() { return net.fabricmc.api.EnvType.CLIENT; }
    public boolean isDevelopmentEnvironment() { return false; }
}

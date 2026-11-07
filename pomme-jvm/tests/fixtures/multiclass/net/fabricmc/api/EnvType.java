package net.fabricmc.api;
public enum EnvType {
    CLIENT, SERVER;
    public boolean isClient() { return this == CLIENT; }
    public boolean isServer() { return this == SERVER; }
}

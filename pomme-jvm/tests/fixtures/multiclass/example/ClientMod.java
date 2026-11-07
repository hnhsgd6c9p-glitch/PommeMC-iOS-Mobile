package example;
import net.fabricmc.api.ClientModInitializer;
import pomme.api.Pomme;
public final class ClientMod implements ClientModInitializer {
    public void onInitializeClient() { Pomme.log("client entrypoint executed"); }
}

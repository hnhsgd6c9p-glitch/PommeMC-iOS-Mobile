package example;

import net.fabricmc.api.ClientModInitializer;
import net.minecraft.client.Minecraft;
import pomme.api.Pomme;

public final class MinecraftVersionProbe implements ClientModInitializer {
    @Override public void onInitializeClient() {
        Pomme.log(Minecraft.getInstance().getLaunchedVersion());
    }
}

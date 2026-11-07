package example;
import net.fabricmc.loader.api.entrypoint.PreLaunchEntrypoint;
import pomme.api.Pomme;
public final class PreLaunchMod implements PreLaunchEntrypoint {
    public void onPreLaunch() { Pomme.log("pre-launch entrypoint executed"); }
}

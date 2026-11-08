package net.minecraft;

import pomme.bridge.PommeGameBridge;

/** Initial intermediary Entity compatibility surface. */
public class class_1297 {
    public double method_23317() { return PommeGameBridge.getDouble(0); }
    public double method_23318() { return PommeGameBridge.getDouble(1); }
    public double method_23321() { return PommeGameBridge.getDouble(2); }
    public boolean method_24828() { return PommeGameBridge.getBoolean(11); }

    public double getX() { return method_23317(); }
    public double getY() { return method_23318(); }
    public double getZ() { return method_23321(); }
    public boolean isOnGround() { return method_24828(); }
}

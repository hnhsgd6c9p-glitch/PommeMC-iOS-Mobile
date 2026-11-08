package net.minecraft;

import pomme.bridge.PommeGameBridge;

/** Initial intermediary LivingEntity compatibility surface. */
public class class_1309 extends class_1297 {
    public float method_6032() { return PommeGameBridge.getFloat(3); }
    public float method_6063() { return PommeGameBridge.getFloat(4); }
    public float getHealth() { return method_6032(); }
    public float getMaxHealth() { return method_6063(); }
}

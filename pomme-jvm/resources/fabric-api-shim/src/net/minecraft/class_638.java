package net.minecraft;

/** Placeholder client-world API object; world queries are added incrementally. */
public final class class_638 {
    public static final class_638 INSTANCE = new class_638();
    private class_638() { }

    /** Empty-world fallback for Fabric's iterable entity query bridge. */
    public Iterable<?> method_18112() {
        return java.util.Collections.emptyList();
    }
}

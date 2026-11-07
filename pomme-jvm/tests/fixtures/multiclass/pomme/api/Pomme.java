package pomme.api;
public final class Pomme {
    private Pomme() {}
    public static native void log(String message);
    public static native void logInt(int value);
}

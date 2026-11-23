package example;
import pomme.api.Pomme;
public final class Helper {
    public static int sharedCount;
    private int instanceCount;
    private Helper() {}
    public static int add(int left, int right) {
        return left + right;
    }
    public static void logObjectValue() {
        Counter first = new Counter(0, "first");
        Counter second = new Counter(1, "second");
        Value firstValue = first;
        Pomme.log(first.getLabel());
        Pomme.log(second.getLabel());
        Pomme.logInt(firstValue.getValue(0));
        Pomme.logInt(ArrayFixture.castCounter(first));
        Pomme.logInt(second.getValue(0));
        String summary = "values=" + first.getValue(0) + "/" + second.getValue(0);
        Pomme.log(summary);
    }
    public static void logArrayValue() {
        int[] values = new int[2];
        values[0] = 4;
        values[1] = 6;
        Pomme.logInt(ArrayFixture.sum(values));
        Pomme.logInt(ArrayFixture.conditional(3));
        Pomme.logInt(ArrayFixture.conditional(7));
        Pomme.logInt(ArrayFixture.switchValue(3));
        Pomme.logInt(ArrayFixture.switchValue(4));
        Pomme.logInt(ArrayFixture.denseSwitchValue(2));
        Pomme.logInt(ArrayFixture.isString("yes"));
        Pomme.logInt(ArrayFixture.isString(null));
        Pomme.logInt(ArrayFixture.caughtException());
        Pomme.logInt(ArrayFixture.caughtArithmetic(0));
        Pomme.logInt(ArrayFixture.arithmetic(5));
        ArrayFixture.logReferenceArray();
        ArrayFixture.logByteArray();
    }
    public static void logStartupValue() {
        Pomme.logInt(ExampleMod.startupValue);
    }
    public static void logStartupMessage() {
        Pomme.log(ExampleMod.startupMessage);
    }
    public static void emitStartupMessage(String message) {
        Pomme.log(message);
    }
}

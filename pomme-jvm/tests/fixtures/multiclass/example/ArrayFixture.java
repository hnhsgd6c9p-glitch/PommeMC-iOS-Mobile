package example;
public final class ArrayFixture {
    public static void logReferenceArray() {
        String[] messages = new String[1];
        messages[0] = "array reference works";
        pomme.api.Pomme.log(messages[0]);
    }
    public static void logByteArray() {
        byte[] values = new byte[1];
        values[0] = (byte) 255;
        pomme.api.Pomme.logInt(isByteArray(values));
        pomme.api.Pomme.logInt(values[0]);
    }
    public static int isByteArray(Object value) {
        return value instanceof byte[] ? 1 : 0;
    }
    public static int sum(int[] values) {
        int total = 0;
        for (int index = 0; index < values.length; index++) {
            total += values[index];
        }
        return total;
    }
    public static int caughtArithmetic(int value) {
        try {
            return 12 / value;
        } catch (ArithmeticException exception) {
            return 99;
        }
    }
    public static int arithmetic(int value) {
        return ((value << 1) ^ 3) / 2;
    }
    public static int caughtException() {
        try {
            throw new MarkerException();
        } catch (MarkerException exception) {
            return 42;
        }
    }
    public static int castCounter(Object value) {
        return ((Counter) value).getValue(0);
    }
    public static int isString(Object value) {
        return value instanceof String ? 1 : 0;
    }
    public static int switchValue(int value) {
        switch (value) {
            case 1: return 11;
            case 3: return 33;
            case 9: return 99;
            default: return -1;
        }
    }
    public static int denseSwitchValue(int value) {
        switch (value) {
            case 1: return 11;
            case 2: return 22;
            case 3: return 33;
            default: return -1;
        }
    }
    public static int conditional(int value) {
        if (value > 5) {
            return value * 2;
        }
        return value + 1;
    }
}

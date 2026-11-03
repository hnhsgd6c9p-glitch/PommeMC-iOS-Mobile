package fixtures;

import java.util.function.IntUnaryOperator;

public final class DynamicLambda {
    public static IntUnaryOperator incrementer() {
        return value -> value + 1;
    }
}

package fixtures;

public interface Arithmetic {
    static int add(int left, int right) {
        return left + right;
    }

    static int sumTo(int limit) {
        int sum = 0;
        for (int value = 1; value <= limit; value++) {
            sum += value;
        }
        return sum;
    }
}

package example;
public final class Counter extends BaseCounter implements Value {
    private int value;
    private String label;
    public Counter(int adjustment, String label) {
        super();
        this.value = nextValue + adjustment;
        this.label = label;
        nextValue = nextValue + 1;
    }
    @Override public int getValue(int adjustment) {
        return value + adjustment;
    }
    public String getLabel() { return label; }
}

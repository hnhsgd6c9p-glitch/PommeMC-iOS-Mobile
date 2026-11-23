package net.minecraft;

import java.util.Objects;

/** Initial intermediary Identifier implementation used by Fabric API events. */
public final class class_2960 implements Comparable<class_2960> {
    public final String field_13353;
    public final String field_13355;

    public class_2960(String namespace, String path) {
        this.field_13353 = Objects.requireNonNull(namespace);
        this.field_13355 = Objects.requireNonNull(path);
    }

    public static class_2960 method_60655(String namespace, String path) {
        return new class_2960(namespace, path);
    }

    public static class_2960 method_60654(String value) {
        return method_12829(value);
    }

    /** Vanilla's default-namespace identifier factory used by CustomPayload.id. */
    public static class_2960 method_60656(String path) {
        return new class_2960("minecraft", path);
    }

    public static class_2960 method_12829(String value) {
        int separator = value.indexOf(':');
        return separator < 0
            ? new class_2960("minecraft", value)
            : new class_2960(value.substring(0, separator), value.substring(separator + 1));
    }

    public String method_12836() { return field_13353; }
    public String method_12832() { return field_13355; }

    @Override public String toString() { return field_13353 + ":" + field_13355; }
    @Override public boolean equals(Object other) {
        return other instanceof class_2960 id
            && field_13353.equals(id.field_13353)
            && field_13355.equals(id.field_13355);
    }
    @Override public int hashCode() { return Objects.hash(field_13353, field_13355); }
    @Override public int compareTo(class_2960 other) { return toString().compareTo(other.toString()); }
}

package net.minecraft;

import io.netty.buffer.ByteBuf;
import java.util.Objects;

/**
 * Functional subset of vanilla CustomPayload required by Fabric networking.
 * The ABI merger supplies the remaining generated methods and nested types.
 */
public interface class_8710 {
    final class class_9154<T extends class_8710> {
        private final class_2960 comp_2242;

        public class_9154(class_2960 id) {
            this.comp_2242 = Objects.requireNonNull(id, "id");
        }

        public class_2960 comp_2242() {
            return comp_2242;
        }

        @Override
        public String toString() {
            return "class_9154[comp_2242=" + comp_2242 + "]";
        }

        @Override
        public int hashCode() {
            return comp_2242.hashCode();
        }

        @Override
        public boolean equals(Object other) {
            return other instanceof class_9154<?> that
                    && comp_2242.equals(that.comp_2242);
        }
    }

    class_9154<? extends class_8710> method_56479();

    static <T extends class_8710> class_9154<T> method_56483(String path) {
        return new class_9154<>(class_2960.method_60656(path));
    }

    static <B extends ByteBuf, T extends class_8710> class_9139<B, T> method_56484(
            class_9143<B, T> encoder,
            class_9141<B, T> decoder) {
        if (encoder == null) {
            throw new NullPointerException("encoder");
        }
        if (decoder == null) {
            throw new NullPointerException("decoder");
        }
        return new PommePayloadCodec<>(encoder, decoder);
    }
}

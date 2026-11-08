package net.minecraft;

/** Functional adapter used by the mobile CustomPayload codec bridge. */
final class PommePayloadCodec<B, T> implements class_9139<B, T> {
    private final class_9143<B, T> encoder;
    private final class_9141<B, T> decoder;

    PommePayloadCodec(class_9143<B, T> encoder, class_9141<B, T> decoder) {
        this.encoder = encoder;
        this.decoder = decoder;
    }

    @Override
    public void encode(B buffer, T value) {
        encoder.encode(value, buffer);
    }

    @Override
    public T decode(B buffer) {
        return decoder.decode(buffer);
    }
}

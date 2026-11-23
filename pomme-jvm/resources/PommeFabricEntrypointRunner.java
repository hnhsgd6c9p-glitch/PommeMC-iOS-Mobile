import java.lang.reflect.Constructor;
import java.lang.reflect.Method;

/**
 * Small launch bridge used by Pomme to invoke Fabric initializer methods
 * inside the embedded JVM. Fabric Loader itself is responsible for discovery
 * and transformation once the launcher integration is connected.
 */
public final class PommeFabricEntrypointRunner {
    private PommeFabricEntrypointRunner() {}

    public static void main(String[] args) throws Exception {
        if ((args.length & 1) != 0) {
            throw new IllegalArgumentException("Entrypoints must be class/method pairs");
        }

        ClassLoader loader = PommeFabricEntrypointRunner.class.getClassLoader();
        for (int index = 0; index < args.length; index += 2) {
            String descriptor = args[index];
            String className = descriptor;
            String factoryName = null;
            int separator = descriptor.indexOf("::");
            if (separator >= 0) {
                className = descriptor.substring(0, separator);
                factoryName = descriptor.substring(separator + 2);
                if (className.isEmpty() || factoryName.isEmpty()) {
                    throw new IllegalArgumentException("Invalid Fabric entrypoint descriptor: " + descriptor);
                }
            }

            Class<?> entrypointClass = Class.forName(className, true, loader);
            Object instance;
            if (factoryName == null || factoryName.equals("<init>")) {
                Constructor<?> constructor = entrypointClass.getDeclaredConstructor();
                constructor.setAccessible(true);
                instance = constructor.newInstance();
            } else {
                Method factory = entrypointClass.getDeclaredMethod(factoryName);
                if (!java.lang.reflect.Modifier.isStatic(factory.getModifiers())) {
                    throw new IllegalArgumentException("Fabric entrypoint factory must be static: " + descriptor);
                }
                factory.setAccessible(true);
                instance = factory.invoke(null);
                if (instance == null || !entrypointClass.isInstance(instance)) {
                    throw new IllegalStateException("Fabric entrypoint factory returned an incompatible instance: " + descriptor);
                }
            }

            Method initializer = entrypointClass.getDeclaredMethod(args[index + 1]);
            initializer.setAccessible(true);
            initializer.invoke(instance);
        }
    }
}

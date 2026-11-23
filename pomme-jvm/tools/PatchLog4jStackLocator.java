import java.io.InputStream;
import java.io.OutputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.Enumeration;
import java.util.jar.JarEntry;
import java.util.jar.JarFile;
import java.util.jar.JarOutputStream;

import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.Opcodes;
import org.objectweb.asm.Type;
import org.objectweb.asm.tree.AbstractInsnNode;
import org.objectweb.asm.tree.ClassNode;
import org.objectweb.asm.tree.FieldInsnNode;
import org.objectweb.asm.tree.LdcInsnNode;
import org.objectweb.asm.tree.MethodNode;

/**
 * Makes Log4j 2.22's caller-class lookup safe on Java 21 interpreters that
 * do not expose the removed sun.reflect.Reflection VM hook.
 */
public final class PatchLog4jStackLocator {
    private static final String CLASS_ENTRY =
            "org/apache/logging/log4j/util/StackLocator.class";
    private static final String OWNER =
            "org/apache/logging/log4j/util/StackLocator";

    private PatchLog4jStackLocator() {
    }

    public static void main(String[] args) throws Exception {
        if (args.length != 1) {
            throw new IllegalArgumentException("Usage: PatchLog4jStackLocator <log4j-api.jar>");
        }

        Path jar = Path.of(args[0]).toAbsolutePath();
        Path temporary = jar.resolveSibling(jar.getFileName() + ".pomme-tmp");
        int patchedReferences = 0;

        try (JarFile input = new JarFile(jar.toFile());
             JarOutputStream output = new JarOutputStream(Files.newOutputStream(temporary))) {
            Enumeration<JarEntry> entries = input.entries();
            while (entries.hasMoreElements()) {
                JarEntry entry = entries.nextElement();
                output.putNextEntry(new JarEntry(entry.getName()));
                try (InputStream stream = input.getInputStream(entry)) {
                    if (CLASS_ENTRY.equals(entry.getName())) {
                        byte[] original = stream.readAllBytes();
                        ClassNode type = new ClassNode(Opcodes.ASM9);
                        new ClassReader(original).accept(type, 0);

                        for (MethodNode method : type.methods) {
                            if (!method.name.equals("getCallerClass")
                                    || !method.desc.equals("(I)Ljava/lang/Class;")) {
                                continue;
                            }
                            for (AbstractInsnNode instruction = method.instructions.getFirst();
                                 instruction != null; ) {
                                AbstractInsnNode next = instruction.getNext();
                                if (instruction instanceof FieldInsnNode field
                                        && field.getOpcode() == Opcodes.GETSTATIC
                                        && field.owner.equals(OWNER)
                                        && field.name.equals("DEFAULT_CALLER_CLASS")
                                        && field.desc.equals("Ljava/lang/Class;")) {
                                    method.instructions.set(
                                            instruction,
                                            new LdcInsnNode(Type.getObjectType("java/lang/Object"))
                                    );
                                    patchedReferences++;
                                }
                                instruction = next;
                            }
                        }

                        if (patchedReferences != 2) {
                            throw new IllegalStateException(
                                    "Expected two Log4j DEFAULT_CALLER_CLASS fallbacks, patched "
                                            + patchedReferences
                            );
                        }

                        ClassWriter writer = new ClassWriter(ClassWriter.COMPUTE_MAXS);
                        type.accept(writer);
                        output.write(writer.toByteArray());
                    } else {
                        stream.transferTo(output);
                    }
                }
                output.closeEntry();
            }
        } catch (Throwable error) {
            Files.deleteIfExists(temporary);
            throw error;
        }

        Files.move(temporary, jar, StandardCopyOption.REPLACE_EXISTING);
        System.out.println("Patched " + patchedReferences
                + " Log4j caller-class fallbacks for the Ristretto Java 21 runtime: " + jar);
    }
}

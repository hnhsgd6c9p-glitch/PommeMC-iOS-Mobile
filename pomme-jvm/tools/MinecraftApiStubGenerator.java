import java.io.*;
import java.nio.file.*;
import java.util.*;
import java.util.jar.*;
import java.util.zip.*;
import org.objectweb.asm.*;

/**
 * Builds a metadata-only Minecraft ABI JAR. It preserves class hierarchy,
 * fields, method descriptors and access flags, but replaces all method bodies
 * with inert defaults. No Mojang method implementation is copied.
 */
public final class MinecraftApiStubGenerator {
  static final class Meta {
    String name, parent, parentCtor;
  }
  static final Set<String> OVERRIDES = new HashSet<>();
  static int generated, skipped;

  static Meta inspect(byte[] bytes) {
    Meta m = new Meta();
    new ClassReader(bytes).accept(new ClassVisitor(Opcodes.ASM9) {
      public void visit(int v,int a,String n,String s,String p,String[] it){m.name=n;m.parent=p;}
      public MethodVisitor visitMethod(int a,String n,String d,String s,String[] ex) {
        if (!n.equals("<init>")) return null;
        return new MethodVisitor(Opcodes.ASM9) {
          public void visitMethodInsn(int op,String owner,String name,String desc,boolean itf) {
            if (m.parentCtor==null && op==Opcodes.INVOKESPECIAL && name.equals("<init>") && owner.equals(m.parent)) m.parentCtor=desc;
          }
        };
      }
    },0);
    if (m.parentCtor==null && "java/lang/Object".equals(m.parent)) m.parentCtor="()V";
    return m;
  }

  static void pushDefault(MethodVisitor mv, Type t) {
    switch(t.getSort()) {
      case Type.BOOLEAN,Type.BYTE,Type.CHAR,Type.SHORT,Type.INT -> mv.visitInsn(Opcodes.ICONST_0);
      case Type.FLOAT -> mv.visitInsn(Opcodes.FCONST_0);
      case Type.LONG -> mv.visitInsn(Opcodes.LCONST_0);
      case Type.DOUBLE -> mv.visitInsn(Opcodes.DCONST_0);
      case Type.ARRAY,Type.OBJECT -> mv.visitInsn(Opcodes.ACONST_NULL);
      default -> throw new IllegalArgumentException("Unsupported constructor argument "+t);
    }
  }

  static byte[] stub(byte[] original, Meta meta) {
    ClassReader reader=new ClassReader(original);
    ClassWriter writer=new ClassWriter(ClassWriter.COMPUTE_MAXS);
    ClassVisitor visitor=new ClassVisitor(Opcodes.ASM9,writer) {
      public MethodVisitor visitMethod(int access,String name,String desc,String signature,String[] exceptions) {
        int safeAccess=access;
        if ((safeAccess & Opcodes.ACC_NATIVE)!=0) safeAccess &= ~Opcodes.ACC_NATIVE;
        MethodVisitor mv=super.visitMethod(safeAccess,name,desc,signature,exceptions);
        if ((safeAccess & Opcodes.ACC_ABSTRACT)!=0) return mv;
        return new MethodVisitor(Opcodes.ASM9,mv) {
          public void visitEnd() {
            mv.visitCode();
            if (name.equals("<init>")) {
              mv.visitVarInsn(Opcodes.ALOAD,0);
              String parentDesc=meta.parentCtor==null ? "()V" : meta.parentCtor;
              for(Type arg:Type.getArgumentTypes(parentDesc)) pushDefault(mv,arg);
              mv.visitMethodInsn(Opcodes.INVOKESPECIAL,meta.parent,"<init>",parentDesc,false);
              mv.visitInsn(Opcodes.RETURN);
            } else if (meta.name.equals("net/minecraft/class_1309")
                && name.equals("method_6116")
                && desc.equals("(Lnet/minecraft/class_1304;Lnet/minecraft/class_1799;Lnet/minecraft/class_1799;)V")) {
              // Preserve the field-read shape used by Lithium's LivingEntityMixin injection.
              // The real method body is deliberately not copied into Pomme's ABI bridge.
              mv.visitVarInsn(Opcodes.ALOAD,0);
              mv.visitFieldInsn(
                  Opcodes.GETFIELD,
                  "net/minecraft/class_1309",
                  "field_5953",
                  "Z"
              );
              mv.visitInsn(Opcodes.POP);
              mv.visitInsn(Opcodes.RETURN);
            } else if (meta.name.equals("net/minecraft/class_1309")
                && name.equals("method_30128")
                && desc.equals("()V")) {
              // Preserve the INVOKE target used by Lithium's equipment-change mixin.
              mv.visitVarInsn(Opcodes.ALOAD,0);
              mv.visitInsn(Opcodes.ACONST_NULL);
              mv.visitMethodInsn(
                  Opcodes.INVOKESPECIAL,
                  "net/minecraft/class_1309",
                  "method_30121",
                  "(Ljava/util/Map;)V",
                  false
              );
              mv.visitInsn(Opcodes.RETURN);
            } else if (meta.name.equals("net/minecraft/class_1309")
                && name.equals("method_6053")
                && desc.equals("()V")) {
              // Preserve Lithium's INVOKE_ASSIGN + local-capture target in updateFallFlying.
              mv.visitVarInsn(Opcodes.ALOAD,0);
              mv.visitInsn(Opcodes.ICONST_0);
              mv.visitMethodInsn(
                  Opcodes.INVOKEVIRTUAL,
                  "net/minecraft/class_1309",
                  "method_5795",
                  "(I)Z",
                  false
              );
              mv.visitVarInsn(Opcodes.ISTORE,1);
              // Lithium captures this boolean local after the INVOKE_ASSIGN shift.
              Label localStart = new Label();
              Label localEnd = new Label();
              mv.visitLabel(localStart);
              mv.visitInsn(Opcodes.NOP);
              mv.visitLabel(localEnd);
              mv.visitInsn(Opcodes.RETURN);
              mv.visitLocalVariable("isFlying","Z",null,localStart,localEnd,1);
            } else {
              Type ret=Type.getReturnType(desc);
              switch(ret.getSort()) {
                case Type.VOID -> mv.visitInsn(Opcodes.RETURN);
                case Type.BOOLEAN,Type.BYTE,Type.CHAR,Type.SHORT,Type.INT -> {mv.visitInsn(Opcodes.ICONST_0);mv.visitInsn(Opcodes.IRETURN);}
                case Type.FLOAT -> {mv.visitInsn(Opcodes.FCONST_0);mv.visitInsn(Opcodes.FRETURN);}
                case Type.LONG -> {mv.visitInsn(Opcodes.LCONST_0);mv.visitInsn(Opcodes.LRETURN);}
                case Type.DOUBLE -> {mv.visitInsn(Opcodes.DCONST_0);mv.visitInsn(Opcodes.DRETURN);}
                case Type.ARRAY,Type.OBJECT -> {mv.visitInsn(Opcodes.ACONST_NULL);mv.visitInsn(Opcodes.ARETURN);}
                default -> throw new IllegalStateException("Unsupported return type "+ret);
              }
            }
            mv.visitMaxs(0,0);
            mv.visitEnd();
          }
        };
      }
    };
    reader.accept(visitor,ClassReader.SKIP_CODE|ClassReader.SKIP_DEBUG|ClassReader.SKIP_FRAMES);
    return writer.toByteArray();
  }

  static boolean wanted(String name) {
    return name.startsWith("net/minecraft/") || name.startsWith("com/mojang/");
  }

  public static void main(String[] args) throws Exception {
    if(args.length<2) throw new IllegalArgumentException("Usage: MinecraftApiStubGenerator <intermediary-client.jar> <output.jar> [override-shim.jar]");
    Path input=Path.of(args[0]), output=Path.of(args[1]);
    if(args.length>2) try(JarFile overrides=new JarFile(args[2])) {
      var es=overrides.entries();while(es.hasMoreElements()){var e=es.nextElement();if(e.getName().endsWith(".class"))OVERRIDES.add(e.getName().substring(0,e.getName().length()-6));}
    }
    Files.createDirectories(output.toAbsolutePath().getParent());
    try(JarFile source=new JarFile(input.toFile());OutputStream file=Files.newOutputStream(output);ZipOutputStream zip=new ZipOutputStream(file)) {
      zip.setLevel(Deflater.BEST_SPEED);
      var entries=source.entries();
      while(entries.hasMoreElements()) {
        JarEntry e=entries.nextElement();
        if(!e.getName().endsWith(".class")) continue;
        try(InputStream in=source.getInputStream(e)) {
          byte[] bytes=in.readAllBytes();
          Meta meta=inspect(bytes);
          if(!wanted(meta.name)){skipped++;continue;}
          byte[] generatedBytes=stub(bytes,meta);
          zip.putNextEntry(new ZipEntry(meta.name+".class"));
          zip.write(generatedBytes);
          zip.closeEntry();
          generated++;
        }
      }
    }
    System.out.println("Generated "+generated+" metadata-only Minecraft/Mojang ABI classes; skipped "+skipped+" non-target/overridden classes.");
    System.out.println("Output: "+output+" ("+Files.size(output)+" bytes)");
  }
}
import java.io.*;
import java.nio.file.*;
import java.util.*;
import java.util.jar.*;
import java.util.zip.*;
import org.objectweb.asm.*;
import org.objectweb.asm.tree.*;

/** Merges generated ABI members into hand-written Pomme bridge classes without replacing their behavior. */
public final class MinecraftApiStubMerger {
  static String fieldKey(FieldNode f){return f.name+f.desc;}
  static String methodKey(MethodNode m){return m.name+m.desc;}

  static byte[] merge(byte[] handBytes,byte[] generatedBytes) {
    ClassNode hand=new ClassNode(Opcodes.ASM9), generated=new ClassNode(Opcodes.ASM9);
    new ClassReader(handBytes).accept(hand,0);
    new ClassReader(generatedBytes).accept(generated,0);
    Set<String> fields=new HashSet<>(),methods=new HashSet<>();
    for(FieldNode f:hand.fields)fields.add(fieldKey(f));
    for(MethodNode m:hand.methods)methods.add(methodKey(m));
    boolean sameSuperclass=Objects.equals(hand.superName,generated.superName);
    int addedFields=0,addedMethods=0;
    for(FieldNode f:generated.fields)if(fields.add(fieldKey(f))){hand.fields.add(f);addedFields++;}
    for(MethodNode m:generated.methods){
      if(m.name.equals("<clinit>"))continue;
      if(m.name.equals("<init>")&&!sameSuperclass)continue;
      if(methods.add(methodKey(m))){hand.methods.add(m);addedMethods++;}
    }
    ClassWriter writer=new ClassWriter(ClassWriter.COMPUTE_MAXS);
    hand.accept(writer);
    return writer.toByteArray();
  }

  public static void main(String[] args)throws Exception {
    if(args.length!=3)throw new IllegalArgumentException("Usage: MinecraftApiStubMerger <generated.jar> <hand-bridge.jar> <output.jar>");
    Path generated=Path.of(args[0]),hand=Path.of(args[1]),output=Path.of(args[2]);
    Map<String,byte[]> entries=new LinkedHashMap<>();
    try(JarFile j=new JarFile(generated.toFile())) {
      var es=j.entries();while(es.hasMoreElements()){var e=es.nextElement();if(!e.isDirectory())try(InputStream in=j.getInputStream(e)){entries.put(e.getName(),in.readAllBytes());}}
    }
    int merged=0,added=0;
    try(JarFile j=new JarFile(hand.toFile())) {
      var es=j.entries();while(es.hasMoreElements()){
        var e=es.nextElement();if(e.isDirectory())continue;
        try(InputStream in=j.getInputStream(e)){
          byte[] bytes=in.readAllBytes();
          if(e.getName().endsWith(".class")&&entries.containsKey(e.getName())){
            entries.put(e.getName(),merge(bytes,entries.get(e.getName())));merged++;
          }else{entries.put(e.getName(),bytes);added++;}
        }
      }
    }
    Files.createDirectories(output.toAbsolutePath().getParent());
    try(ZipOutputStream zip=new ZipOutputStream(Files.newOutputStream(output))){
      zip.setLevel(Deflater.BEST_SPEED);
      for(var e:entries.entrySet()){zip.putNextEntry(new ZipEntry(e.getKey()));zip.write(e.getValue());zip.closeEntry();}
    }
    System.out.println("Merged generated ABI members into "+merged+" hand-written bridge classes; added "+added+" hand-bridge resources/classes.");
    System.out.println("Output: "+output+" ("+Files.size(output)+" bytes)");
  }
}
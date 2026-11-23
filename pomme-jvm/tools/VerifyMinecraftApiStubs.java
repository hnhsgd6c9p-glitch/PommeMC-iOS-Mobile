import java.net.*;
import java.nio.file.*;
import java.util.*;
import java.util.jar.*;

/** Verifies every generated Minecraft ABI class and resolves all field/method descriptors. */
public final class VerifyMinecraftApiStubs {
  public static void main(String[] args) throws Exception {
    if(args.length!=2)throw new IllegalArgumentException("Usage: VerifyMinecraftApiStubs <bridge.jar> <lib-dir>");
    Path bridge=Path.of(args[0]),lib=Path.of(args[1]);
    List<URL> urls=new ArrayList<>();urls.add(bridge.toUri().toURL());
    try(var jars=Files.newDirectoryStream(lib,"*.jar")){for(Path jar:jars)urls.add(jar.toUri().toURL());}
    int loaded=0,failed=0;
    try(JarFile archive=new JarFile(bridge.toFile());
        URLClassLoader loader=new URLClassLoader(urls.toArray(URL[]::new),ClassLoader.getPlatformClassLoader())) {
      var entries=archive.entries();
      while(entries.hasMoreElements()) {
        var entry=entries.nextElement();
        if(!entry.getName().endsWith(".class"))continue;
        String name=entry.getName().substring(0,entry.getName().length()-6).replace('/','.');
        try {
          Class<?> type=Class.forName(name,false,loader);
          type.getDeclaredFields();type.getDeclaredMethods();type.getDeclaredConstructors();
          type.getInterfaces();type.getSuperclass();loaded++;
        } catch(Throwable error) {
          failed++;
          if(failed<=40)System.err.println("ABI class verification failed: "+name+" -> "+error);
        }
      }
    }
    System.out.println("Verified "+loaded+" Minecraft/Fabric ABI classes and their member descriptors; failures: "+failed);
    if(failed!=0)System.exit(1);
  }
}
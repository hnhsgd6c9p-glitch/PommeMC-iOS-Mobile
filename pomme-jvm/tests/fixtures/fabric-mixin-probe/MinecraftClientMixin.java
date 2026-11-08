package probe.mixin;

import net.minecraft.class_310;
import com.llamalad7.mixinextras.injector.wrapmethod.WrapMethod;
import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import org.spongepowered.asm.mixin.Mixin;

@Mixin(class_310.class)
public abstract class MinecraftClientMixin {
    @WrapMethod(method = "isPaused()Z")
    private boolean pomme$wrapPaused(Operation<Boolean> original) {
        return true;
    }
}

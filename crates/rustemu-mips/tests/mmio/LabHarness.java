import java.lang.reflect.*;
import java.util.*;
import java.awt.event.MouseListener;
import javax.swing.*;
import mars.*;
import mars.mips.hardware.*;
import mars.tools.DigitalLabSim;

/** Runs a program with MARS's Digital Lab Sim connected, without showing its window.
 *  Arguments: file, clicks ("step:key,..." - key 0..15), steps. Prints the displays, the key
 *  pressed, the registers, Status / Cause / EPC, the bytes 0xffff0010..0xffff0017 and the error. */
public class LabHarness {
    static Object call(Object o, String name) throws Exception {
        for (Class<?> c = o.getClass(); c != null; c = c.getSuperclass()) {
            try {
                Method m = c.getDeclaredMethod(name);
                m.setAccessible(true);
                return m.invoke(o);
            } catch (NoSuchMethodException e) {}
        }
        throw new NoSuchMethodException(name);
    }
    static Object field(Class<?> c, Object o, String name) throws Exception {
        Field f = c.getDeclaredField(name);
        f.setAccessible(true);
        return f.get(o);
    }

    public static void main(String[] a) throws Exception {
        System.setProperty("apple.awt.UIElement", "true");
        String file = a[0];
        TreeMap<Integer, List<Integer>> clicks = new TreeMap<>();
        if (!a[1].isEmpty()) for (String k : a[1].split(",")) {
            String[] p = k.split(":");
            clicks.computeIfAbsent(Integer.parseInt(p[0]), x -> new ArrayList<>()).add(Integer.parseInt(p[1]));
        }
        int steps = Integer.parseInt(a[2]);

        Globals.initialize(false);
        DigitalLabSim tool = new DigitalLabSim("t", "h");
        call(tool, "buildMainDisplayArea");
        MIPSprogram code = new MIPSprogram();
        ArrayList<String> files = new ArrayList<>();
        files.add(file);
        code.assemble(code.prepareFilesForAssembly(files, file, null), true, false);
        RegisterFile.initializeProgramCounter(false);
        call(tool, "addAsObserver");

        Object keys = field(DigitalLabSim.class, tool, "hexaKeyPanel");
        JButton[] buttons = (JButton[]) field(keys.getClass(), keys, "button");
        String error = null;
        int done = 0;
        try {
            boolean finished = false;
            for (Map.Entry<Integer, List<Integer>> e : clicks.entrySet()) {
                int at = Math.min(e.getKey(), steps);
                if (at > done && !finished) finished = code.simulate(at - done);
                done = at;
                for (int k : e.getValue())
                    for (MouseListener l : buttons[k].getMouseListeners())
                        if (l.getClass().getName().contains("EcouteurClick")) l.mouseClicked(null);
            }
            if (steps > done && !finished) code.simulate(steps - done);
        } catch (ProcessingException pe) {
            error = pe.errors().generateErrorReport().trim();
        }
        Object panel = field(DigitalLabSim.class, tool, "sevenSegPanel");
        Object[] displays = (Object[]) field(panel.getClass(), panel, "display");
        int left = (char) field(displays[0].getClass(), displays[0], "aff") & 0xff;
        int right = (char) field(displays[1].getClass(), displays[1], "aff") & 0xff;
        System.out.println("SEG " + Integer.toHexString(left) + " " + Integer.toHexString(right));
        System.out.println("KEY " + field(DigitalLabSim.class, null, "KeyBoardValueButtonClick"));
        for (int r = 0; r < 32; r++) System.out.println("R" + r + " " + Integer.toHexString(RegisterFile.getValue(r)));
        System.out.println("PC " + Integer.toHexString(RegisterFile.getProgramCounter()));
        for (int r : new int[] {12, 13, 14}) System.out.println("C" + r + " " + Integer.toHexString(Coprocessor0.getValue(r)));
        for (int i = 0; i < 2; i++) System.out.println("M" + i + " " + Integer.toHexString(Globals.memory.getRawWord(0xffff0010 + 4 * i)));
        if (error != null) System.out.println("ERROR " + error.replace('\n', ' '));
        System.exit(0);
    }
}

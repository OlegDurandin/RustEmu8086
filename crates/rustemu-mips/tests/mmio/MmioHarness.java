import java.lang.reflect.*;
import java.util.*;
import java.awt.event.KeyEvent;
import java.awt.event.KeyListener;
import javax.swing.*;
import mars.*;
import mars.mips.hardware.*;
import mars.tools.KeyboardAndDisplaySimulator;

/** Runs a program with MARS's Keyboard and Display MMIO Simulator connected, without a window.
 *  Arguments: file, delay index (0..40), display after delay (0/1), keys ("step:code,..."),
 *  steps. Prints the terminal size, the display text (escaped), the registers and the error. */
public class MmioHarness {
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
    static Object field(Object o, String name) throws Exception {
        for (Class<?> c = o.getClass(); c != null; c = c.getSuperclass()) {
            try {
                Field f = c.getDeclaredField(name);
                f.setAccessible(true);
                return f.get(o);
            } catch (NoSuchFieldException e) {}
        }
        throw new NoSuchFieldException(name);
    }
    static void setField(Object o, String name, Object v) throws Exception {
        Field f = o.getClass().getDeclaredField(name);
        f.setAccessible(true);
        f.set(o, v);
    }

    public static void main(String[] a) throws Exception {
        System.setProperty("apple.awt.UIElement", "true");
        String file = a[0];
        int delayIndex = Integer.parseInt(a[1]);
        boolean dad = a[2].equals("1");
        TreeMap<Integer, List<Integer>> keys = new TreeMap<>();
        if (!a[3].isEmpty()) for (String k : a[3].split(",")) {
            String[] p = k.split(":");
            keys.computeIfAbsent(Integer.parseInt(p[0]), x -> new ArrayList<>()).add(Integer.parseInt(p[1]));
        }
        int steps = Integer.parseInt(a[4]);

        Globals.initialize(false);
        KeyboardAndDisplaySimulator tool = new KeyboardAndDisplaySimulator("t", "h");
        call(tool, "initializePreGUI");
        call(tool, "buildMainDisplayArea");
        JTextArea display = (JTextArea) field(tool, "display");
        display.setSize(600, 300);
        JSlider slider = (JSlider) field(tool, "delayLengthSlider");
        slider.setValue(delayIndex);
        setField(tool, "displayAfterDelay", dad);
        java.awt.Dimension size = (java.awt.Dimension) call(tool, "getDisplayPanelTextDimensions");
        System.out.println("SIZE " + (int) size.getWidth() + " " + (int) size.getHeight());

        MIPSprogram code = new MIPSprogram();
        ArrayList<String> files = new ArrayList<>();
        files.add(file);
        ArrayList progs = code.prepareFilesForAssembly(files, file, null);
        code.assemble(progs, true, false);
        RegisterFile.initializeProgramCounter(false);
        call(tool, "addAsObserver");
        call(tool, "initializePostGUI");

        JTextArea keyArea = (JTextArea) field(tool, "keyEventAccepter");
        KeyListener listener = null;
        for (KeyListener l : keyArea.getKeyListeners())
            if (l.getClass().getName().contains("KeyboardKeyListener")) listener = l;
        String error = null;
        int done = 0;
        try {
            boolean finished = false;
            for (Map.Entry<Integer, List<Integer>> e : keys.entrySet()) {
                int at = Math.min(e.getKey(), steps);
                if (at > done && !finished) finished = code.simulate(at - done);
                done = at;
                for (int c : e.getValue())
                    listener.keyTyped(new KeyEvent(keyArea, KeyEvent.KEY_TYPED, 0, 0, KeyEvent.VK_UNDEFINED, (char) c));
            }
            if (steps > done && !finished) code.simulate(steps - done);
        } catch (ProcessingException pe) {
            error = pe.errors().generateErrorReport().trim();
        }
        StringBuilder t = new StringBuilder();
        for (char c : display.getText().toCharArray()) {
            if (c >= 32 && c < 127 && c != '\\') t.append(c); else t.append(String.format("\\x%02x", (int) c));
        }
        System.out.println("DISPLAY " + t);
        for (int r = 0; r < 32; r++) System.out.println("R" + r + " " + Integer.toHexString(RegisterFile.getValue(r)));
        System.out.println("PC " + Integer.toHexString(RegisterFile.getProgramCounter()));
        for (int r : new int[] {12, 13, 14}) System.out.println("C" + r + " " + Integer.toHexString(Coprocessor0.getValue(r)));
        for (int i = 0; i < 4; i++) System.out.println("M" + i + " " + Integer.toHexString(Globals.memory.getRawWord(0xffff0000 + 4 * i)));
        if (error != null) System.out.println("ERROR " + error.replace('\n', ' '));
        System.exit(0);
    }
}

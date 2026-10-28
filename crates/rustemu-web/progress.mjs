// Trunk initializer (`data-initializer` in index.html): shows how much of the wasm has arrived
// while the page loads. The app removes #rustemu_status once it runs (src/main.rs).
export default function progress() {
  const status = () => document.getElementById("rustemu_status");
  const show = (text, fraction) => {
    const s = status();
    if (!s) return;
    s.querySelector(".text").textContent = text;
    if (fraction !== undefined) s.querySelector(".bar > div").style.width = `${Math.round(fraction * 100)}%`;
  };
  return {
    onProgress: ({ current, total }) => {
      if (total > 0) {
        const f = Math.min(current / total, 1);
        show(`Loading RustEmu8086… ${Math.round(f * 100)}%`, f);
      } else {
        show(`Loading RustEmu8086… ${(current / 1048576).toFixed(1)} MB`);
      }
    },
    onComplete: () => show("Starting…", 1),
    onFailure: (error) => {
      show(`RustEmu8086 could not load: ${error}`);
      status()?.querySelector(".bar")?.remove();
    },
  };
}

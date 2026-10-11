<script lang="ts">
  let {
    checked,
    onchange,
    disabled = false,
    label,
    busy = false,
  }: { checked: boolean; onchange: (v: boolean) => void; disabled?: boolean; label: string; busy?: boolean } = $props();
</script>

<button
  type="button"
  class="toggle"
  class:on={checked}
  class:busy
  role="switch"
  aria-checked={checked}
  aria-label={label}
  title={label}
  disabled={disabled || busy}
  aria-busy={busy}
  onclick={(e) => {
    e.stopPropagation();
    onchange(!checked);
  }}
>
  <span class="knob"></span>
</button>

<style>
  .toggle {
    position: relative;
    flex: none;
    width: 30px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 9px;
    background: color-mix(in srgb, var(--text-3) 45%, transparent);
    transition: background-color var(--dur) ease;
  }
  .toggle.on {
    background: var(--tint);
  }
  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: white;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.25);
    transition: transform 220ms var(--ease-spring);
  }
  .on .knob {
    transform: translateX(12px);
  }
  .busy .knob {
    animation: wobble 0.9s ease-in-out infinite;
  }
  @keyframes wobble {
    50% { opacity: 0.55; }
  }
  .toggle:disabled {
    opacity: 0.4;
  }
</style>

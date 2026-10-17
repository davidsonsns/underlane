<script lang="ts">
  import ArrowUpCircle from "@lucide/svelte/icons/circle-arrow-up";
  import { onMount } from "svelte";
  import { AppUpdate } from "$lib/app-update.svelte";
  import Tip from "$lib/components/app/Tip.svelte";
  import {
    Cancel as AlertCancel,
    Content as AlertContent,
    Description as AlertDescription,
    Footer as AlertFooter,
    Header as AlertHeader,
    Root as AlertRoot,
    Title as AlertTitle,
  } from "$lib/components/ui/alert-dialog/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import { returnFocusTo } from "$lib/focus";
  import { store } from "$lib/store.svelte";
  import { notify } from "$lib/toast";

  const updater = new AppUpdate();
  const labels = {
    available: "Update available",
    checking: "",
    downloading: "Downloading…",
    idle: "",
    installing: "Restarting…",
    ready: "Restart to update",
  };
  let confirmRestart = $state(false);
  let trigger = $state<HTMLButtonElement | null>(null);
  const busy = $derived(["downloading", "installing"].includes(updater.status));

  onMount(() => {
    // Development builds must never replace the installed app with a published build.
    if (import.meta.env.DEV) {
      return;
    }
    updater.check();
    const timer = setInterval(() => updater.check(), 6 * 60 * 60 * 1000);
    return () => {
      clearInterval(timer);
      updater.dispose();
    };
  });

  async function install() {
    confirmRestart = false;
    try {
      await updater.install();
    } catch (error) {
      notify.error(error, "Could not update Underlane");
    }
  }

  async function activate() {
    if (updater.status === "ready") {
      if (store.tunnels.some((tunnel) => tunnel.state.status !== "stopped")) {
        confirmRestart = true;
      } else {
        await install();
      }
      return;
    }
    try {
      await updater.download();
    } catch (error) {
      notify.error(error, "Could not download update");
    }
  }
</script>

{#if updater.status !== "idle" && updater.status !== "checking"}
  <Tip text={`Underlane ${updater.version}`}>
    {#snippet children({
      props,
    })}
      <Button
        {...props}
        aria-busy={busy || undefined}
        class="h-8 w-full justify-start gap-2.5 bg-primary/8 text-primary hover:bg-primary/12 hover:text-primary"
        disabled={busy}
        onclick={activate}
        variant="ghost"
        bind:ref={trigger}
      >
        {#if busy}
          <Spinner aria-hidden="true" class="size-[15px]" />
        {:else}
          <ArrowUpCircle class="size-[15px]" />
        {/if}
        <span aria-live="polite">
          {labels[updater.status]}{updater.status === "downloading" &&
          updater.progress !== null
            ? ` ${updater.progress}%`
            : ""}
        </span>
      </Button>
    {/snippet}
  </Tip>
{/if}

<AlertRoot bind:open={confirmRestart}>
  <AlertContent onCloseAutoFocus={returnFocusTo(() => trigger)}>
    <AlertHeader>
      <AlertTitle>Restart to update Underlane?</AlertTitle>
      <AlertDescription>
        Underlane {updater.version} is ready. Running projects reconnect after a
        brief interruption; stopped projects stay stopped. Temporary quick links
        receive new URLs.
      </AlertDescription>
    </AlertHeader>
    <AlertFooter>
      <AlertCancel>Later</AlertCancel>
      <Button onclick={install}>Restart and update</Button>
    </AlertFooter>
  </AlertContent>
</AlertRoot>

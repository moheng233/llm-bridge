import { type Ref } from "vue";
export function useUnsavedChanges(dirty: Ref<boolean>, busy: Ref<boolean>) {
  const confirm = useConfirm();
  let pending: Promise<boolean> | null = null;
  async function confirmDiscard() {
    if (busy.value) return false;
    if (!dirty.value) return true;
    if (pending) return pending;
    pending = confirm({
      title: "放弃未保存的修改？",
      description: "已提交的提供者、模型和连接不会被撤销。未保存的密钥和模型草稿将丢失。",
      confirmText: "放弃修改",
    });
    try {
      return await pending;
    } finally {
      pending = null;
    }
  }
  function beforeUnload(event: BeforeUnloadEvent) {
    if (dirty.value || busy.value) {
      event.preventDefault();
      event.returnValue = "";
    }
  }
  onBeforeRouteLeave(confirmDiscard);
  onMounted(() => window.addEventListener("beforeunload", beforeUnload));
  onBeforeUnmount(() => window.removeEventListener("beforeunload", beforeUnload));
  return { confirmDiscard };
}

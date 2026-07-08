// 全局确认对话框 composable — 替代原 useConfirm.svelte.ts
// 仅用于删除场景（启用/禁用、改角色直接执行）。
//
// 用法：
//   import { useConfirm } from '~/composables/useConfirm'
//   const confirm = useConfirm()
//   const ok = await confirm({
//     title: "确认删除",
//     description: "确定要删除模型 X 吗？该操作不可撤销。",
//     confirmText: "确认删除",
//     destructive: true,
//   })
//   if (ok) { ... }

export interface ConfirmOptions {
  title: string;
  description?: string;
  confirmText?: string;
  cancelText?: string;
  destructive?: boolean;
}

interface ConfirmState extends ConfirmOptions {
  resolve: (v: boolean) => void;
  open: boolean;
}

const current = ref<ConfirmState | null>(null);

export function useConfirm() {
  return function confirm(opts: ConfirmOptions): Promise<boolean> {
    if (current.value?.open) {
      current.value.resolve(false);
    }
    return new Promise<boolean>((resolve) => {
      current.value = {
        ...opts,
        resolve,
        open: true,
      };
    });
  };
}

export function getConfirmState() {
  return current;
}

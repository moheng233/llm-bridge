// 全局确认对话框 hook — 见 PLAN.md §9 决策⑦。
// 仅用于删除场景（启用/禁用、改角色直接执行）。
//
// 用法：
//   import { confirm } from "$lib/hooks/useConfirm";
//   const ok = await confirm({
//     title: "确认删除",
//     description: "确定要删除模型 X 吗？该操作不可撤销。",
//     confirmText: "确认删除",
//     destructive: true,
//   });
//   if (ok) { ... }
//
// 实现：单一全局 promise + 一个挂载在 App.svelte 的 <ConfirmHost /> 组件。

import type { Component } from "svelte";

export interface ConfirmOptions {
  title: string;
  description?: string;
  /** 确认按钮文案，默认"确认" */
  confirmText?: string;
  /** 取消按钮文案，默认"取消" */
  cancelText?: string;
  /** 是否危险操作（红色确认按钮），默认 false */
  destructive?: boolean;
}

interface ConfirmState extends ConfirmOptions {
  resolve: (v: boolean) => void;
  open: boolean;
}

let current = $state<ConfirmState | null>(null);

/**
 * 触发一个全局确认对话框，返回 Promise<boolean>。
 * 同一时刻只允许一个确认框（后到的会等待前一个关闭）。
 */
export function confirm(opts: ConfirmOptions): Promise<boolean> {
  // 若已有打开的确认框，先把旧的解析为 false（避免悬挂）
  if (current?.open) {
    current.resolve(false);
  }
  return new Promise<boolean>((resolve) => {
    current = {
      ...opts,
      resolve,
      open: true,
    };
  });
}

/** 内部：供 ConfirmHost 调用，关闭并解决 promise */
function resolveConfirm(value: boolean) {
  if (current) {
    current.resolve(value);
    current.open = false;
    // 延迟清空，等淡出动画结束（这里简化为立即清空状态对象）
    current = null;
  }
}

export const confirmState = {
  get current() {
    return current;
  },
  get open() {
    return current?.open ?? false;
  },
  resolve: resolveConfirm,
};

/** 暴露给 ConfirmHost 组件用的状态 */
export function getConfirmState() {
  return confirmState;
}

<script setup lang="ts">
import MarkdownIt from "markdown-it";

// 安全默认值：禁用原始 HTML（防 XSS），只渲染 markdown 语法
const md = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: false,
});

const props = defineProps<{ text: string; class?: string }>();

const html = computed(() => md.render(props.text));
</script>

<template>
  <!-- eslint-disable-next-line vue/no-v-html : html=false 已禁原始 HTML，输出安全 -->
  <div data-slot="markdown-text" :class="['md-body', props.class]" v-html="html" />
</template>

<style>
/* 非 scoped：v-html 内容不带 data 属性，需全局选择器。
   类名加前缀避免污染外部。 */
.md-body {
  font-size: 0.875rem;
  line-height: 1.7;
  color: var(--foreground);
}

.md-body > :first-child {
  margin-top: 0;
}
.md-body > :last-child {
  margin-bottom: 0;
}

.md-body p {
  margin: 0.5em 0;
}

.md-body h1,
.md-body h2,
.md-body h3,
.md-body h4 {
  margin: 0.9em 0 0.4em;
  font-weight: 600;
  line-height: 1.3;
}
.md-body h1 {
  font-size: 1.15rem;
}
.md-body h2 {
  font-size: 1.05rem;
}
.md-body h3,
.md-body h4 {
  font-size: 0.95rem;
}

.md-body ul,
.md-body ol {
  margin: 0.5em 0;
  padding-left: 1.4em;
  display: flex;
  flex-direction: column;
  gap: 0.15em;
}
.md-body ul {
  list-style: disc;
}
.md-body ol {
  list-style: decimal;
}

/* 行内代码 */
.md-body code:not(pre code) {
  font-family: var(--font-mono);
  font-size: 0.8em;
  background: color-mix(in oklab, var(--foreground) 8%, transparent);
  border-radius: 4px;
  padding: 0.1em 0.35em;
}

/* 代码块 */
.md-body pre {
  margin: 0.6em 0;
  padding: 0.7em 0.9em;
  border-radius: var(--radius-md);
  border: 1px solid var(--border);
  background: var(--muted);
  overflow-x: auto;
}
.md-body pre code {
  font-family: var(--font-mono);
  font-size: 0.78rem;
  line-height: 1.55;
  background: transparent;
  padding: 0;
  color: var(--foreground);
}

.md-body blockquote {
  margin: 0.5em 0;
  padding-left: 0.9em;
  border-left: 3px solid var(--border);
  color: var(--muted-foreground);
}

.md-body a {
  color: var(--color-cta);
  text-decoration: underline;
  text-underline-offset: 2px;
}

.md-body strong {
  font-weight: 600;
}

.md-body hr {
  margin: 0.8em 0;
  border-color: var(--border);
}

.md-body table {
  margin: 0.5em 0;
  border-collapse: collapse;
  font-size: 0.8rem;
}
.md-body th,
.md-body td {
  border: 1px solid var(--border);
  padding: 0.3em 0.7em;
}
.md-body th {
  background: var(--muted);
  font-weight: 600;
}
</style>

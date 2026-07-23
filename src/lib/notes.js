// Small pure helpers shared across the note UI.

export const debounce = (fn, waitMs = 350) => {
  let t = null;

  const wrapped = (...args) => {
    if (t) clearTimeout(t);
    t = setTimeout(() => {
      t = null;
      fn(...args);
    }, waitMs);
  };

  wrapped.cancel = () => {
    if (!t) return;
    clearTimeout(t);
    t = null;
  };

  wrapped.flush = (...args) => {
    if (t) {
      clearTimeout(t);
      t = null;
    }
    return fn(...args);
  };

  return wrapped;
};

const normalizeMarkdown = (markdown) => String(markdown ?? "").replace(/\r\n/g, "\n");

const escapeHtml = (value) =>
  String(value ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");

const syntax = (value) => `<span class="md-syntax">${escapeHtml(value)}</span>`;

const renderInlineMarkdownHTML = (value) => {
  let html = escapeHtml(value);

  html = html.replace(
    /!\[([^\]]*)\]\(([^)]*)\)/g,
    (_match, alt, url) =>
      `${syntax("![")}<span class="md-link-label">${alt}</span>${syntax(`](${url})`)}`,
  );

  html = html.replace(
    /\[([^\]]+)\]\(([^)]*)\)/g,
    (_match, label, url) =>
      `${syntax("[")}<span class="md-link-label">${label}</span>${syntax(`](${url})`)}`,
  );

  html = html.replace(
    /`([^`]+)`/g,
    (_match, code) =>
      `${syntax("`")}<span class="md-inline-code">${code}</span>${syntax("`")}`,
  );

  html = html.replace(
    /~~([^~]+)~~/g,
    (_match, text) =>
      `${syntax("~~")}<span class="md-strike">${text}</span>${syntax("~~")}`,
  );

  html = html.replace(
    /\*\*([^*]+)\*\*/g,
    (_match, text) =>
      `${syntax("**")}<span class="md-strong">${text}</span>${syntax("**")}`,
  );

  html = html.replace(
    /__([^_]+)__/g,
    (_match, text) =>
      `${syntax("__")}<span class="md-strong">${text}</span>${syntax("__")}`,
  );

  html = html.replace(
    /(^|[^*])\*([^*]+)\*(?!\*)/g,
    (_match, before, text) =>
      `${before}${syntax("*")}<span class="md-em">${text}</span>${syntax("*")}`,
  );

  html = html.replace(
    /(^|[^_])_([^_]+)_(?!_)/g,
    (_match, before, text) =>
      `${before}${syntax("_")}<span class="md-em">${text}</span>${syntax("_")}`,
  );

  return html;
};

// Derive a card/editor title from the first non-empty line of markdown.
export const firstLineTitle = (markdown) => {
  const text = normalizeMarkdown(markdown).trim();
  if (!text) return "Untitled";
  const firstLine = text.split("\n")[0].trim();
  if (!firstLine) return "Untitled";
  return firstLine.replace(/^#{1,6}\s+/, "").slice(0, 80) || "Untitled";
};

// Everything after the title line — used as the notification sub-text.
export const noteBody = (markdown) => {
  const lines = normalizeMarkdown(markdown).split("\n");
  // Skip leading blank lines, then the title line itself.
  let i = 0;
  while (i < lines.length && lines[i].trim() === "") i++;
  i++;
  return lines.slice(i).join("\n").trim();
};

const stripInlineMarkdown = (value) =>
  String(value ?? "")
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/(\*\*|__)(.*?)\1/g, "$2")
    .replace(/(\*|_)(.*?)\1/g, "$2")
    .replace(/~~(.*?)~~/g, "$1");

const toNotificationLine = (line) => {
  let text = String(line ?? "");
  text = text.replace(/^\s*#{1,6}\s+/, "");
  text = text.replace(
    /^(\s*)[-*+]\s+\[( |x|X)\]\s+/,
    (_match, indent, checked) => `${indent}${checked.trim() ? "☑ " : "☐ "}`,
  );
  text = text.replace(/^(\s*)[-*+]\s+/, "$1• ");
  text = text.replace(/^(\s*)>\s?/, "$1");
  return stripInlineMarkdown(text);
};

// Notification main text: the title with markdown syntax stripped.
export const notificationTitle = (markdown) =>
  stripInlineMarkdown(firstLineTitle(markdown));

// Notification sub-text: the body with markdown syntax removed and list dashes
// turned into bullet points, matching how the editor renders it.
export const notificationBody = (markdown) =>
  noteBody(markdown)
    .split("\n")
    .map(toNotificationLine)
    .join("\n")
    .trim();

export const renderMarkdownPreviewLines = (markdown) => {
  const lines = normalizeMarkdown(markdown).split("\n");
  const rendered = [];
  let inFence = false;

  for (const line of lines) {
    const fenceMatch = line.match(/^(\s*```.*)$/);
    if (fenceMatch) {
      inFence = !inFence;
      rendered.push({
        className: "md-line md-line--fence",
        html: syntax(fenceMatch[1]),
      });
      continue;
    }

    if (inFence) {
      rendered.push({
        className: "md-line md-line--code",
        html: escapeHtml(line) || "&nbsp;",
      });
      continue;
    }

    if (line.trim() === "") {
      rendered.push({
        className: "md-line md-line--blank",
        html: "&nbsp;",
      });
      continue;
    }

    let match = line.match(/^(\s*)(#{1,6})(\s+)(.*)$/);
    if (match) {
      const [, indent, hashes, gap, text] = match;
      rendered.push({
        className: `md-line md-line--heading md-line--h${hashes.length}`,
        html:
          escapeHtml(indent) +
          syntax(hashes) +
          escapeHtml(gap) +
          renderInlineMarkdownHTML(text),
      });
      continue;
    }

    match = line.match(/^(\s*)[-*+]\s+\[( |x|X)\]\s+(.*)$/);
    if (match) {
      const [, indent, checked, text] = match;
      rendered.push({
        className: `md-line md-line--task ${checked.trim() ? "is-checked" : ""}`.trim(),
        html:
          escapeHtml(indent) +
          `<span class="md-task-box">${checked.trim() ? "☑" : "☐"}</span> ` +
          renderInlineMarkdownHTML(text),
      });
      continue;
    }

    match = line.match(/^(\s*)[-*+](\s+)(.*)$/);
    if (match) {
      const [, indent, gap, text] = match;
      rendered.push({
        className: "md-line md-line--list",
        html:
          escapeHtml(indent) +
          '<span class="md-list-bullet">•</span>' +
          escapeHtml(gap) +
          renderInlineMarkdownHTML(text),
      });
      continue;
    }

    match = line.match(/^(\s*)(\d+)([.)])(\s+)(.*)$/);
    if (match) {
      const [, indent, number, punctuation, gap, text] = match;
      rendered.push({
        className: "md-line md-line--list md-line--ordered",
        html:
          escapeHtml(indent) +
          `<span class="md-list-number">${escapeHtml(number + punctuation)}</span>` +
          escapeHtml(gap) +
          renderInlineMarkdownHTML(text),
      });
      continue;
    }

    match = line.match(/^(\s*)>\s?(.*)$/);
    if (match) {
      const [, indent, text] = match;
      rendered.push({
        className: "md-line md-line--quote",
        html:
          escapeHtml(indent) +
          '<span class="md-quote-marker">│</span> ' +
          renderInlineMarkdownHTML(text),
      });
      continue;
    }

    rendered.push({
      className: "md-line md-line--paragraph",
      html: renderInlineMarkdownHTML(line),
    });
  }

  return rendered;
};

// Short preview of the note body for the card.
export const previewText = (markdown) => {
  const text = normalizeMarkdown(markdown).split("\n").slice(0, 6).join("\n").trim();
  return text ? text.slice(0, 240) : "Click to edit…";
};

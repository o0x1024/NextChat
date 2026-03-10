import ReactMarkdown from "react-markdown";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";

interface MarkdownMessageProps {
  content: string;
}

export function MarkdownMessage({ content }: MarkdownMessageProps) {
  return (
    <div className="chat-markdown">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkBreaks]}
        components={{
          a: ({ node: _node, ...props }) => (
            <a
              {...props}
              className="link link-primary break-all"
              rel="noreferrer"
              target="_blank"
            />
          ),
          code: ({ children, className, ...props }) => {
            const isBlock = Boolean(className);

            if (!isBlock) {
              return (
                <code
                  {...props}
                  className="rounded bg-base-100/70 px-1 py-0.5 font-mono text-[0.85em]"
                >
                  {children}
                </code>
              );
            }

            return (
              <code {...props} className={className}>
                {children}
              </code>
            );
          },
          pre: ({ node: _node, ...props }) => (
            <pre
              {...props}
              className="my-3 overflow-x-auto rounded-box bg-base-100/80 p-3 font-mono text-xs"
            />
          ),
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}

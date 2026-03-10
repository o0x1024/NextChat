import ReactMarkdown from "react-markdown";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";

interface MarkdownMessageProps {
  content: string;
}

export function MarkdownMessage({ content }: MarkdownMessageProps) {
  return (
    <div className="chat-markdown min-w-0 max-w-full">
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
                  className="rounded px-1 py-0.5 font-mono text-[0.85em]"
                >
                  {children}
                </code>
              );
            }

            return (
              <code {...props} className={`${className} block text-inherit`}>
                {children}
              </code>
            );
          },
          pre: ({ node: _node, ...props }) => (
            <pre
              {...props}
              className="my-3 min-w-0 max-w-full whitespace-pre-wrap break-words rounded-box p-3 font-mono text-xs leading-relaxed shadow-sm"
            />
          ),
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}

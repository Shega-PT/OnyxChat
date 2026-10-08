import React, { useEffect, useRef } from 'react';
import OnyxMessage from '@/components/onyx/OnyxMessage';
import { formatDayLabel } from '@/lib/onyx/format';

function DaySeparator({ label }) {
  return (
    <div className="my-3 flex items-center gap-3">
      <span className="h-px flex-1 bg-onyx-line" />
      <span className="onyx-label">{label}</span>
      <span className="h-px flex-1 bg-onyx-line" />
    </div>
  );
}

export default function MessageList({ messages = [], isGroup = false }) {
  const containerRef = useRef(null);

  useEffect(() => {
    const element = containerRef.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [messages.length]);

  let lastDay = null;

  return (
    <div ref={containerRef} className="min-h-0 flex-1 overflow-y-auto px-4 py-2">
      <div className="mx-auto flex max-w-[860px] flex-col gap-1.5">
        {messages.map((message) => {
          const day = formatDayLabel(message.at);
          const showDay = day !== lastDay;
          lastDay = day;
          return (
            <React.Fragment key={message.id}>
              {showDay && <DaySeparator label={day} />}
              <OnyxMessage message={message} showAuthor={isGroup} />
            </React.Fragment>
          );
        })}
        <div className="h-2" />
      </div>
    </div>
  );
}
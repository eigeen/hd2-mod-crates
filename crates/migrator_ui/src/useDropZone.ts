import { type DragEventHandler, useCallback, useRef, useState } from "react";

interface DropZoneHandlers {
  onDragEnter: DragEventHandler<HTMLDivElement>;
  onDragLeave: DragEventHandler<HTMLDivElement>;
  onDragOver: DragEventHandler<HTMLDivElement>;
  onDrop: DragEventHandler<HTMLDivElement>;
}

interface DropZoneState {
  dragging: boolean;
  handlers: DropZoneHandlers;
}

export function useDropZone(
  receiveDrop: (dataTransfer: DataTransfer) => void | Promise<void>,
): DropZoneState {
  const [dragging, setDragging] = useState(false);
  const depth = useRef(0);
  const onDragEnter = useCallback(() => enterDropZone(depth, setDragging), []);
  const onDragLeave = useCallback(() => leaveDropZone(depth, setDragging), []);
  const onDragOver = useCallback<DragEventHandler<HTMLDivElement>>(allowDrop, []);
  const onDrop = useCallback<DragEventHandler<HTMLDivElement>>((event) => {
    event.preventDefault();
    resetDropZone(depth, setDragging);
    void receiveDrop(event.dataTransfer);
  }, [receiveDrop]);
  return { dragging, handlers: { onDragEnter, onDragLeave, onDragOver, onDrop } };
}

function allowDrop(event: React.DragEvent<HTMLDivElement>): void {
  event.preventDefault();
  event.dataTransfer.dropEffect = "copy";
}

function enterDropZone(
  depth: React.MutableRefObject<number>,
  setDragging: (value: boolean) => void,
): void {
  depth.current += 1;
  setDragging(true);
}

function leaveDropZone(
  depth: React.MutableRefObject<number>,
  setDragging: (value: boolean) => void,
): void {
  depth.current = Math.max(0, depth.current - 1);
  if (depth.current === 0) setDragging(false);
}

function resetDropZone(
  depth: React.MutableRefObject<number>,
  setDragging: (value: boolean) => void,
): void {
  depth.current = 0;
  setDragging(false);
}

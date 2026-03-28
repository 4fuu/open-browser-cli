export interface Request {
  id: string;
  action: string;
  params: Record<string, unknown>;
}

export interface Response {
  id: string;
  ok: boolean;
  data?: Record<string, unknown>;
  error?: string;
}

export interface Session {
  session_id: string;
  tab_id: number;
  url: string;
  title: string;
  created_at: number;
  status: 'active' | 'loading' | 'closed';
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface RawNode {
  ref: string;
  parent?: string;
  tag: string;
  text: string;
  attrs: Record<string, string>;
  rect: Rect;
}

export interface SnapshotMeta {
  url: string;
  title: string;
  viewport: { width: number; height: number };
  scroll: { top: number; height: number };
}

export interface RawSnapshot extends SnapshotMeta {
  nodes: RawNode[];
}

export interface PageChunk {
  type: 'page_chunk';
  session_id: string;
  request_id: string;
  meta?: SnapshotMeta;
  nodes: RawNode[];
  chunk_index: number;
  done: boolean;
}

export interface ContentRequest {
  type: 'snapshot' | 'click' | 'type' | 'wait' | 'presence_start' | 'presence_stop' | 'resolve_url';
  params: Record<string, unknown>;
}

export interface ContentResponse {
  ok: boolean;
  data?: Record<string, unknown>;
  error?: string;
}

export interface DownloadChunk {
  type: 'download_chunk';
  session_id: string;
  request_id: string;
  chunk_index: number;
  data: string;
  done: boolean;
  filename?: string;
  content_type?: string;
  size?: number;
}

export interface ChunkEvent {
  type: 'page_chunk' | 'download_chunk';
  chunk: PageChunk | DownloadChunk;
}

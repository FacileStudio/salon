import * as decoding from 'lib0/decoding';
import { WebsocketProvider } from 'y-websocket';
import type * as Y from 'yjs';
import { connectionOf, MESSAGE_CUSTOM, stopOf, type Connection, type Stop } from './codes';

/** Where a room lives: the y-websocket base URL and the room name that completes it. */
export interface RoomTarget {
	base: string;
	room: string;
	protocols?: string[];
	params?: Record<string, string>;
}

/** What a page hears from its room. `onroomclose` fires on 1001, before the reconnect. */
export interface RoomEvents {
	onconnection: (state: Connection) => void;
	onstop: (stop: Stop, reason: string) => void;
	onsynced?: () => void;
	onroomclose?: () => void;
	oncustom?: (notice: unknown) => void;
}

/** An open room: the provider (presence at `provider.awareness`) and how to close it. */
export interface Room {
	provider: WebsocketProvider;
	destroy: () => void;
}

type Handler = (typeof WebsocketProvider.prototype.messageHandlers)[number];

function customHandler(events: RoomEvents): Handler {
	const read = (_encoder: unknown, decoder: decoding.Decoder) => {
		let notice: unknown = null;
		try {
			notice = JSON.parse(decoding.readVarString(decoder));
		} catch {
			notice = null;
		}
		events.oncustom?.(notice);
	};
	return read as unknown as Handler;
}

/** Opens `target` over y-websocket 3 with no BroadcastChannel: stops on 1008, 1009 and 1011, reconnects otherwise. */
export function openRoom(target: RoomTarget, ydoc: Y.Doc, events: RoomEvents): Room {
	const provider = new WebsocketProvider(target.base, target.room, ydoc, {
		disableBc: true,
		protocols: target.protocols,
		params: target.params ?? {},
		shouldReconnect: (event: CloseEvent) => stopOf(event.code) === null
	});
	provider.messageHandlers[MESSAGE_CUSTOM] = customHandler(events);
	let status = 'connecting';
	let stopped = false;
	const report = () =>
		events.onconnection(stopped ? 'stopped' : connectionOf(status, provider.synced, navigator.onLine));
	provider.on('status', (event: { status: string }) => {
		status = event.status;
		report();
	});
	provider.on('sync', (synced: boolean) => {
		if (synced) events.onsynced?.();
		report();
	});
	provider.on('connection-close', (event: CloseEvent | null) => {
		if (event?.code === 1001) events.onroomclose?.();
	});
	provider.on('closed', (event: { code: number; reason: string }) => {
		const stop = stopOf(event.code);
		if (!stop) return;
		stopped = true;
		report();
		events.onstop(stop, event.reason);
	});
	window.addEventListener('online', report);
	window.addEventListener('offline', report);
	report();
	return {
		provider,
		destroy: () => {
			window.removeEventListener('online', report);
			window.removeEventListener('offline', report);
			provider.destroy();
		}
	};
}

// See https://svelte.dev/docs/kit/types#app.d.ts
// for information about these interfaces
import type { Diagnostic } from '$lib/api/generated/Diagnostic';
import type { ViewSummary } from '$lib/api/generated/ViewSummary';

declare global {
	namespace App {
		interface Error {
			message: string;
			diagnostics?: Diagnostic[];
		}
		interface PageData {
			views?: ViewSummary[];
			viewsStatus?: number;
			layoutDiagnostics?: Diagnostic[];
		}
		// interface Locals {}
		// interface PageState {}
		// interface Platform {}
	}
}

export {};

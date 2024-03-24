import type { Actions } from './$types';

export const actions = {
    default: async (event) => {
        return {
            components: [
                {
                    gate: "userland",
                    name: "web/curl",
                    version: "8.3.0",
                    revison: "0",
                }
            ]
        }
    },
} satisfies Actions;
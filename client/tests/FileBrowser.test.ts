import { renderWithProviders } from './renderWithProviders';
import FileBrowser from '../src/components/file-browser/FileBrowser.vue';
import { vi } from 'vitest';

vi.mock('../src/services/files.service', () => ({
  filesService: {
    getFiles: vi.fn(async () => []),
  },
}));

describe('FileBrowser.vue', () => {
  it('shows empty folder message when no files', async () => {
    // jsdom: mock matchMedia
    vi.stubGlobal('matchMedia', (q: string) => ({
      matches: false,
      media: q,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }));

    const { findByText } = renderWithProviders(FileBrowser as any);
    await findByText('此文件夹为空');
  });
});

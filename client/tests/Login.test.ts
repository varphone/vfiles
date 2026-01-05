import Login from '../src/views/Login.vue';
import { renderWithProviders } from './renderWithProviders';

describe('Login.vue', () => {
  it('renders login form by default', async () => {
    const { getByLabelText, getByText } = renderWithProviders(Login);

    expect(getByText('VFiles 登录')).toBeTruthy();
    // username input exists
    expect(getByLabelText(/用户名/i)).toBeTruthy();
    // password input exists
    expect(getByLabelText(/密码/i)).toBeTruthy();
  });
});

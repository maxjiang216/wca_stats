import './globals.css';
import Sidebar from '@/components/Sidebar';

export const metadata = {
  title: 'WCA Stats',
  description: 'Derived statistics from World Cube Association results',
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>
        <div className="layout">
          <Sidebar />
          <main className="content">{children}</main>
        </div>
      </body>
    </html>
  );
}

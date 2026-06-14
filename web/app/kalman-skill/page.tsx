import KalmanSkillTable from '@/components/KalmanSkillTable';

export default function KalmanSkillPage() {
  return (
    <>
      <div className="page-header">
        <h1>Kalman Skill</h1>
        <p className="desc">
          A state-space model of each competitor&apos;s latent skill, per event (tabs above the table), in log-time, tracked
          weekly with a Kalman filter and RTS smoother (the exact linear-Gaussian equivalent of
          TrueSkill-Through-Time). Skill is a <em>damped local linear trend</em> — a level plus an
          improvement rate that bends toward a plateau — with robust (Student-t) handling of outlier
          weeks and a coupled filter tracking each person&apos;s solve-to-solve spread (CV). Unlike
          the EWMA Skill Estimator, this carries uncertainty, so a Monte-Carlo ao5 simulator gives
          expected ao5, P(sub-X), and record probabilities. Skill (single) is the smoothed mean solve
          time projected to the current week; Trend is the weekly improvement rate (green = improving).
        </p>
      </div>
      <KalmanSkillTable />
    </>
  );
}

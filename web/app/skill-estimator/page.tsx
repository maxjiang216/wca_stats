import SkillEstimatorTable from '@/components/SkillEstimatorTable';

export default function SkillEstimatorPage() {
  return (
    <>
      <div className="page-header">
        <h1>Skill Estimator</h1>
        <p className="desc">
          Exponentially weighted moving average of each competitor&apos;s mean non-DNF solve time
          across all rounds at each competition. The decay parameter (half-life) is chosen
          separately per event to minimise weighted prediction error on held-out competitions.
          Lower estimate = faster current estimated skill.
        </p>
      </div>
      <SkillEstimatorTable />
    </>
  );
}

#ifndef SYSTEM_H
#define SYSTEM_H

// Glicko system; contains hyperparameters

class System {

  float start_sigma2, start_rho, start_nu2, sigma2_const, nu2_const, min_rho,
      min_sigma2, min_nu2;

public:
  System(float start_sigma2, float start_rho, float start_nu2,
         float sigma2_const, float nu2_const, float min_rho,
         float min_sigma2, float min_nu2);
  ~System() = default;

  friend class Person;
};

#endif
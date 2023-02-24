#ifndef SYSTEM_H
#define SYSTEM_H

// Glicko system; contains hyperparameters

class System {

  double start_sigma2, start_rho, start_nu2, sigma2_const, nu2_const, min_rho,
      min_sigma2, min_nu2;

public:
  System(double start_sigma2, double start_rho, double start_nu2,
         double sigma2_const, double nu2_const, double min_rho,
         double min_sigma2, double min_nu2);
  ~System() = default;

  friend class Person;
};

#endif
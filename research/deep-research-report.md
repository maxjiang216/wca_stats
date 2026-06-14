# State-space models: local level, linear trend, damped trend

We consider discrete-time Gaussian state-space models with state vector $x_t$ and observation $y_t$.  Each model is expressed as 
$$y_t = Z_t x_t + \varepsilon_t,\quad \varepsilon_t\sim N(0,R_t),$$ 
$$x_{t+1} = G_t x_t + w_t,\quad w_t\sim N(0,Q_t).$$ 
Here $Z_t$ is the observation row-vector and $G_t$ the state transition matrix (both can be time-varying).  For our three models, we use a state vector $x_t$ that includes the *level* $\ell_t$ and (for trend models) the *slope* $b_t$.  We set $Z_t=[1\ 0]$ (observing only $\ell_t$).  The models are: 

- **Local level** (random walk plus noise): $x_t=[\ell_t]$, 
  $y_t=\ell_t+\varepsilon_t,\ \ell_{t+1}=\ell_t+\eta_t$, 
  with $\varepsilon_t\sim N(0,\sigma^2_\varepsilon)$, $\eta_t\sim N(0,\sigma^2_\eta)$.  In matrix form $G_t=[1]$, $Z_t=[1]$, $Q_t=[\sigma^2_\eta]$, $R_t=[\sigma^2_\varepsilon]$. 

- **Local linear trend**: $x_t=[\ell_t,\;b_t]'$, 
  $$y_t=\ell_t+\varepsilon_t,\quad \ell_{t+1}=\ell_t+b_t+\eta_t,\quad b_{t+1}=b_t+\xi_t,$$ 
  with $\varepsilon_t\sim N(0,\sigma^2_\varepsilon)$, $\eta_t\sim N(0,\sigma^2_\eta)$, $\xi_t\sim N(0,\sigma^2_\xi)$.  Equivalently $G_t=\begin{pmatrix}1&1\\0&1\end{pmatrix}$, $Z_t=[1\ 0]$, $Q_t=\mathrm{diag}(\sigma^2_\eta,\sigma^2_\xi)$.  

- **Damped local linear trend**: like the above but the slope is *damped* by a factor $0<\phi<1$.  We set 
  $$\ell_{t+1}=\ell_t+\phi\,b_t+\eta_t,\quad b_{t+1}=\phi\,b_t+\xi_t,$$ 
  so $G_t=\begin{pmatrix}1&\phi\\0&\phi\end{pmatrix}$ with $0<\phi<1$ reducing the impact of the previous slope (at $\phi=1$ it reverts to the undamped trend).  The errors are $w_t=[\eta_t,\xi_t]'$ with $E[w_tw_t']=Q_t=\mathrm{diag}(\sigma^2_\eta,\sigma^2_\xi)$, and $Z_t=[1\ 0]$ as before.  (This corresponds to Holt’s *damped trend* in ETS models.) 

In all cases we allow $R_t$ (the measurement noise variance) to vary with $t$ if needed.  We assume independent Gaussian noises.  Initial state $x_1$ is diffuse or given by a prior mean and large covariance.

## Kalman filter (predict–update)

Given parameters $(G_t,Z_t,Q_t,R_t)$ and an initial estimate $\hat x_{1|0}$, $P_{1|0}$, the *Kalman filter* recursions proceed for $t=1,\dots,T$: 

1. **Predict:** 
   $$\hat x_{t|t-1} = G_{t-1}\,\hat x_{t-1|t-1},\quad 
     P_{t|t-1} = G_{t-1}\,P_{t-1|t-1}\,G_{t-1}' + Q_{t-1}.$$ 
   (If $t=1$, use the initial $G_0$ update.)  

2. **Observation prediction:** 
   $$\hat y_{t|t-1} = Z_t\,\hat x_{t|t-1},\quad 
      F_t = Z_t\,P_{t|t-1}\,Z_t' + R_t.$$ 

3. **Update (if $y_t$ is observed):**  
   Compute the *innovation* (prediction error) $\nu_t = y_t - \hat y_{t|t-1}$ and the Kalman gain  
   $$K_t = P_{t|t-1}\,Z_t'\,F_t^{-1}.$$  
   Then update the state estimate and covariance:  
   $$\hat x_{t|t} = \hat x_{t|t-1} + K_t\,\nu_t,\quad 
     P_{t|t} = (I - K_t Z_t)\,P_{t|t-1}\,(I - K_t Z_t)' + K_t R_t K_t'.$$  
   The **Joseph form** of the covariance update (above) is numerically stable and equivalent to the simpler $P_{t|t}=(I-K_tZ_t)P_{t|t-1}$.  

If $y_t$ is missing at time $t$, we simply skip the update step: the filter produces $\hat x_{t|t}=\hat x_{t|t-1}$, $P_{t|t}=P_{t|t-1}$.  Equivalently, one may implement a *selection matrix* $M_t$ that picks only the observed components (see below).  For time-varying $R_t$, one replaces $R$ by $R_t$ in the $F_t$ and Joseph updates, which poses no extra difficulty (the same recursions apply with $Z_t$ and $R_t$ allowed to change over $t$).  

*Numerical stability:*  In practice one uses the Joseph form above for $P_{t|t}$.  To avoid inversion, one can also factorize $F_t$ (e.g. via Cholesky) when computing $K_t$.  In implementations it is common to iterate over $t$ and only invert or solve linear systems for each scalar variance $F_t$ or small matrix.

## Rauch–Tung–Striebel (RTS) smoother (backward pass)

After running the filter forward, we obtain filtered state means $\hat x_{t|t}$ and covariances $P_{t|t}$ (and predictions $\hat x_{t+1|t},P_{t+1|t}$).  The *RTS smoother* produces smoothed estimates $\hat x_{t|T} = E[x_t\,|\,y_{1:T}]$ for $t=T,\dots,1$ by a backward recursion.  Define the smoothing gain  
$$A_t \;=\; P_{t|t}\,G_t'\,[P_{t+1|t}]^{-1}.$$  
Then initialize $\hat x_{T|T}=\hat x_{T|T}$, $P_{T|T}$, and recurse for $t=T-1,\ldots,1$: 
\[
\hat x_{t|T} = \hat x_{t|t} + A_t\,\bigl(\hat x_{t+1|T}-\hat x_{t+1|t}\bigr), 
\qquad
P_{t|T} = P_{t|t} + A_t\bigl(P_{t+1|T}-P_{t+1|t}\bigr)A_t'.
\]
This yields the minimum-variance smoothers.  (These equations follow from Rauch et al. (1965), or see any standard text.  They are algebraically equivalent to other forms of the backward equations.)  In summary, one first runs the Kalman filter forward, then runs this backward recursion to refine the estimates using future data.  

## Handling missing or irregular observations

If some observations $y_t$ are missing or time intervals irregular, the filter easily adapts.  When $y_t$ is missing, one simply does the predict step but omits the update, i.e.\ $\hat x_{t|t}=\hat x_{t|t-1}$, $P_{t|t}=P_{t|t-1}$.  Equivalently, one can use a *selection matrix* $M_t$ that picks out the observed entries: if only a subset of $y_t$ is available, replace $Z_t$ by $M_t Z_t$ and $R_t$ by $M_t R_t M_t'$ so that the Kalman update is applied only to present data.  This approach handles arbitrarily missing components or grouping of observations.  For irregularly spaced data (time gaps), one can evolve the state over the gap with the same $G$ and $Q$ (possibly scaled by the time difference) and perform no measurement update until the next observation.  In short, missing $y_t$ values lead to predict-only steps, which the Kalman recursion naturally supports.  Time-varying measurement noise $R_t$ is handled simply by using the appropriate $R_t$ in each update (i.e.\ allowing $H_t$ in [57] to vary).

## Parameter estimation by maximum likelihood

We estimate hyperparameters (noise variances $Q_t$, $R_t$ and damping $\phi$) by maximizing the Gaussian log-likelihood of the observed data.  In state-space form, the log-likelihood can be computed via the *prediction-error decomposition*: at each time $t$, the one-step forecast error $\nu_t=y_t-\hat y_{t|t-1}$ is Gaussian with covariance $F_t$.  Thus 
$$\log L(\theta) = -\tfrac12\sum_{t=1}^T\bigl(\log|F_t| + \nu_t'F_t^{-1}\nu_t\bigr) + \text{const},$$ 
where $F_t=Z_tP_{t|t-1}Z_t'+R_t$.  One can maximize this via numerical optimization (e.g. gradient-based methods) by running the Kalman filter to compute $L(\theta)$ and its derivatives w.r.t.\ $\theta$ (by numerical differentiation or with analytic gradients).

Alternatively, one can use an **EM algorithm** (Shumway–Stoffer 1982).  In the EM approach, the *complete data* are the hidden states; one computes their expected sufficient statistics under the current parameter estimates using the Kalman smoother, then updates parameters in closed form.  Concretely, the M-step updates for Gaussian errors yield 
$$\phi,\;Q,\;R \;\leftarrow\; \arg\min \Bigl[\sum_t E[(x_{t+1}-G x_t)(x_{t+1}-G x_t)']\Bigr],\;\arg\min\Bigl[\sum_t E[(y_t-Zx_t)(y_t-Zx_t)']\Bigr],$$ 
with expectations from the smoother.  Shumway and Stoffer show that this leads to simple recursions using the filter and smoother results.  Each EM iteration is guaranteed to increase the likelihood and converge to a stationary point (though possibly slowly).  

By contrast, **direct optimization** of the log-likelihood (via, say, Newton or quasi-Newton methods) can converge faster (especially near the optimum) but may require good initialization and handling of parameter constraints (e.g.\ variances must be positive, $0<\phi<1$).  In practice, one often uses EM for robust initial estimates and then refines by a gradient optimizer.  

For **many short independent series** (e.g.\ forecasting many products), one can exploit parallelism or vectorization.  Each series is independent, so one can run separate Kalman filters (and EM fits) in parallel threads or use batched operations.  If parameters are shared or similar across series, one can pool information (e.g.\ assume common $Q,R$) to gain stability.  Numerical efficiency tips include using small linear algebra libraries (BLAS) for each series, or writing the filter in C/NUMBA to handle millions of tiny filters quickly.  For very short series, one may need to regularize estimates (e.g.\ impose minimum variance) because individual ML estimates can be imprecise.  

## Pseudocode

Below is compact pseudocode outlining (1) the Kalman filter and RTS smoother, and (2) ML parameter fitting via EM or optimization.  Matrix multiplications and inverses are indicated abstractly.  

```python
# Forward Kalman filter (for t=1..T)
x_pred[1] = init_x;  P_pred[1] = init_P
for t=1..T:
    # Predict (skip at t=1 if init was at t=1)
    x_pred[t] = G[t-1] @ x_filt[t-1]
    P_pred[t] = G[t-1] @ P_filt[t-1] @ G[t-1].T + Q[t-1]
    # If observation y[t] is present:
    v = y[t] - Z[t] @ x_pred[t]       # innovation
    S = Z[t] @ P_pred[t] @ Z[t].T + R[t]  # innovation covariance
    K = P_pred[t] @ Z[t].T @ inv(S)       # Kalman gain
    x_filt[t] = x_pred[t] + K @ v
    P_filt[t] = (I - K @ Z[t]) @ P_pred[t]  # or Joseph form
else:
    # no update if y[t] missing
    x_filt[t] = x_pred[t];  P_filt[t] = P_pred[t]
```

```python
# Backward RTS smoother (for t=T-1..1)
x_smooth[T] = x_filt[T];  P_smooth[T] = P_filt[T]
for t=T-1..1:
    J = P_filt[t] @ G[t].T @ inv(P_pred[t+1])
    x_smooth[t] = x_filt[t] + J @ (x_smooth[t+1] - x_pred[t+1])
    P_smooth[t] = P_filt[t] + J @ (P_smooth[t+1] - P_pred[t+1]) @ J.T
```

```python
# (a) EM algorithm for parameter estimation:
init parameters θ
repeat:
    # E-step: run filter+RTS smoother to get E[x_t], E[x_t x_t'], E[x_{t+1}x_t'] for all t
    (x_smooth, P_smooth, P_cross) = KalmanFilterAndRTS(y, θ)
    # M-step: update parameters (depending on model):
    φ = sum(E[b_{t+1} b_t])/sum(E[b_t^2])   # e.g. damped factor
    σ^2_η = average of E[(l_{t+1}-l_t-φ b_t)^2]
    σ^2_ξ = average of E[(b_{t+1}-φ b_t)^2]
    σ^2_ε = average of E[(y_t - l_t)^2]
until convergence

# (b) Direct optimization:
optimize θ to maximize sum(-0.5*(log|S_t| + v_t^T S_t^{-1} v_t)), 
where S_t = Z P_pred[t] Z^T + R[t] and (v_t,P_pred) come from Kalman filter.
```

In these steps, `inv(S)` denotes a matrix inverse (or solved linear system), and expectations like `E[...]` are computed from the smoothed covariances (e.g.\ `P_smooth[t]`).  For millions of small series, one would vectorize or parallelize the loops above, reuse factorizations for repeated matrix inverses (since $Z$ is usually simple), and initialize parameters sensibly (e.g.\ from data variances) to speed convergence.

**Sources:** The model equations and Kalman filter/smoother recurrences are standard (see Durbin & Koopman (2012) or Hyndman & Athanasopoulos (2018) and references therein).  Handling of missing data via a selection matrix is described in Kalman filter references.  The likelihood (prediction-error) form and EM updates follow Shumway & Stoffer (1982), and numerically stable covariance updates use the Joseph form.
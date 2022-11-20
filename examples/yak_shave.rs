//! The yak shaving example from tracing, adapted for use with serif

mod shaving {
    use std::error::Error;

    use serif::macros::*;
    use serif::tracing::{self, Level};
    use thiserror::Error;

    // the `#[tracing::instrument]` attribute creates and enters a span
    // every time the instrumented function is called. The span is named after
    // the function or method. Paramaters passed to the function are recorded as fields.
    #[tracing::instrument]
    pub fn shave(yak: usize) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        // this creates an event at the TRACE log level with two fields:
        // - `excitement`, with the key "excitement" and the value "yay!"
        // - `message`, with the key "message" and the value "hello! I'm gonna shave a yak."
        //
        // unlike other fields, `message`'s shorthand initialization is just the string itself.
        trace!(excitement = "yay!", "hello! I'm gonna shave a yak");
        if yak == 3 {
            warn!("could not locate yak");
            return Err(YakError::MissingYak(MissingYakError::OutOfSpace(
                OutOfSpaceError::OutOfCash,
            ))
            .into());
        } else {
            trace!("yak shaved successfully");
        }
        Ok(())
    }

    pub fn shave_all(yaks: usize) -> usize {
        // Constructs a new span named "shaving_yaks" at the INFO level,
        // and a field whose key is "yaks". This is equivalent to writing:
        //
        // let span = span!(Level::INFO, "shaving_yaks", yaks = yaks);
        //
        // local variables (`yaks`) can be used as field values
        // without an assignment, similar to struct initializers.
        let span = span!(Level::INFO, "shaving_yaks", yaks);
        let _enter = span.enter();

        info!("shaving yaks");

        let mut yaks_shaved = 0;
        for yak in 1..=yaks {
            let res = shave(yak);
            debug!(target: "yak_events", yak, shaved = res.is_ok());

            if let Err(ref error) = res {
                // Like spans, events can also use the field initialization shorthand.
                // In this instance, `yak` is the field being initialized.
                error!(yak, error = error.as_ref(), "failed to shave yak");
            } else {
                yaks_shaved += 1;
            }
            trace!(yaks_shaved);
        }

        yaks_shaved
    }

    // Error types
    #[derive(Debug, Error)]
    enum OutOfSpaceError {
        #[error("out of cash")]
        OutOfCash,
    }

    #[derive(Debug, Error)]
    enum MissingYakError {
        #[error("out of space: {0}")]
        OutOfSpace(#[from] OutOfSpaceError),
    }

    #[derive(Debug, Error)]
    enum YakError {
        #[error("missing yak: {0}")]
        MissingYak(#[from] MissingYakError),
    }
}

fn main() {
    serif::Config::new()
        // set trace level by default to show all the features
        .with_default(tracing::Level::TRACE)
        // these options are the defaults, but are included here for completeness
        .with_output(serif::Output::Stdout)
        .with_color(serif::ColorMode::Auto)
        .with_timestamp(serif::TimeFormat::Local)
        .with_target(true)
        .with_scope(true)
        .init();

    let number_of_yaks = 3;
    // this creates a new event, outside of any spans.
    tracing::info!(number_of_yaks, "preparing to shave yaks");

    let number_shaved = shaving::shave_all(number_of_yaks);
    tracing::info!(all_yaks_shaved = number_shaved == number_of_yaks, "yak shaving completed.");
}
